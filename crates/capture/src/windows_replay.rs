use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use clipforge_common::ContainerFormat;

/// Duration of each rolling replay segment in seconds.
pub const REPLAY_SEGMENT_TIME_SECS: u32 = 2;

/// Computes the bounded number of rolling ring-buffer segment slots for `replay_secs`.
///
/// Strictly bounded to `ceil(replay_secs / 2) + 2` so RAM and disk never grow unbounded.
pub fn compute_replay_ring_slots(replay_secs: u32) -> u32 {
    replay_secs.max(1).div_ceil(REPLAY_SEGMENT_TIME_SECS) + 2
}

/// Orders replay segment paths chronologically (oldest first, newest last) and keeps
/// at most `max_segments` most recent segments.
pub fn order_replay_segments(
    entries: &[(PathBuf, SystemTime)],
    max_segments: usize,
) -> Vec<PathBuf> {
    if max_segments == 0 || entries.is_empty() {
        return Vec::new();
    }
    let mut sorted = entries.to_vec();
    sorted.sort_by(|(pa, ta), (pb, tb)| ta.cmp(tb).then_with(|| pa.cmp(pb)));
    let start = sorted.len().saturating_sub(max_segments);
    sorted.into_iter().skip(start).map(|(p, _)| p).collect()
}

/// Builds an FFmpeg `concat` demuxer manifest from an ordered list of segment paths.
pub fn build_concat_manifest(segments: &[PathBuf]) -> String {
    let mut out = String::new();
    for seg in segments {
        let escaped = seg.to_string_lossy().replace('\'', r"'\''");
        out.push_str(&format!("file '{escaped}'\n"));
    }
    out
}

/// Builds the FFmpeg CLI arguments for concatenating snapshotted replay segments
/// into a playable clip with `-c copy` (and `-movflags +faststart` for MP4).
pub fn build_concat_ffmpeg_args(
    manifest_path: &Path,
    output_path: &Path,
    container: ContainerFormat,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("-y"),
        OsString::from("-hide_banner"),
        OsString::from("-loglevel"),
        OsString::from("warning"),
        OsString::from("-f"),
        OsString::from("concat"),
        OsString::from("-safe"),
        OsString::from("0"),
        OsString::from("-i"),
        manifest_path.as_os_str().to_os_string(),
        OsString::from("-c"),
        OsString::from("copy"),
    ];
    if container == ContainerFormat::Mp4 {
        args.push(OsString::from("-movflags"));
        args.push(OsString::from("+faststart"));
    }
    args.push(output_path.as_os_str().to_os_string());
    args
}

/// Returns the file extension for a [`ContainerFormat`].
pub fn container_extension(container: ContainerFormat) -> &'static str {
    match container {
        ContainerFormat::Mp4 => "mp4",
        ContainerFormat::Mkv => "mkv",
        ContainerFormat::Webm => "webm",
        ContainerFormat::Flv => "flv",
    }
}

#[cfg(target_os = "windows")]
pub mod runtime {
    use std::path::{Path, PathBuf};

    use chrono::Utc;
    use clipforge_common::{CaptureConfig, ClipForgeError, ContainerFormat, Result};
    use tokio::process::Command;
    use tokio::sync::mpsc;

    use super::{
        REPLAY_SEGMENT_TIME_SECS, build_concat_ffmpeg_args, build_concat_manifest,
        compute_replay_ring_slots, container_extension, order_replay_segments,
    };
    use crate::windows_audio::CapturedAudioChunk;
    use crate::windows_encoder::runtime::WindowsEncoderSession;
    use crate::windows_encoder::{CREATE_NO_WINDOW, WindowsOutputMode, find_ffmpeg_binary};
    use crate::windows_wgc::CapturedVideoFrame;

    /// Active Windows rolling-segment replay buffer session.
    ///
    /// Temp segments live inside `temp_dir` and are automatically cleaned up on `stop()` / `Drop`.
    pub struct WindowsReplaySession {
        temp_dir: tempfile::TempDir,
        encoder: WindowsEncoderSession,
        replay_secs: u32,
        container: ContainerFormat,
    }

    impl WindowsReplaySession {
        /// Starts rolling segmented capture into an isolated temporary directory.
        pub async fn start(
            config: &CaptureConfig,
            width: u32,
            height: u32,
            video_rx: mpsc::Receiver<CapturedVideoFrame>,
            loopback_rx: Option<mpsc::Receiver<CapturedAudioChunk>>,
            mic_rx: Option<mpsc::Receiver<CapturedAudioChunk>>,
        ) -> Result<Self> {
            let temp_dir = tempfile::Builder::new()
                .prefix("clipforge-win-replay-")
                .tempdir()
                .map_err(|e| {
                    ClipForgeError::CaptureFailed(format!(
                        "failed to create replay temp directory: {e}"
                    ))
                })?;

            let ext = container_extension(config.container);
            let pattern = temp_dir.path().join(format!("seg_%03d.{ext}"));
            let slots = compute_replay_ring_slots(config.replay_buffer_secs);

            let encoder = WindowsEncoderSession::start(
                config,
                width,
                height,
                WindowsOutputMode::RollingSegment {
                    segment_time_secs: REPLAY_SEGMENT_TIME_SECS,
                    segment_wrap: slots,
                    pattern,
                },
                video_rx,
                loopback_rx,
                mic_rx,
            )
            .await?;

            Ok(Self {
                temp_dir,
                encoder,
                replay_secs: config.replay_buffer_secs,
                container: config.container,
            })
        }

        /// Checks whether the underlying FFmpeg process has exited unexpectedly.
        pub fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
            self.encoder.try_wait()
        }

        /// Snapshots active ring segments in chronological order and concatenates them
        /// into a playable clip inside `output_dir`.
        pub async fn save_replay(&self, output_dir: &Path) -> Result<PathBuf> {
            let ffmpeg = find_ffmpeg_binary().ok_or_else(|| {
                ClipForgeError::CaptureUnavailable(
                    "ffmpeg.exe was not found in the application directory or PATH".to_string(),
                )
            })?;

            tokio::fs::create_dir_all(output_dir).await?;

            let mut entries = Vec::new();
            let mut rd = tokio::fs::read_dir(self.temp_dir.path()).await?;
            while let Some(entry) = rd.next_entry().await? {
                let path = entry.path();
                let Ok(meta) = entry.metadata().await else {
                    continue;
                };
                if !meta.is_file() || meta.len() == 0 {
                    continue;
                }
                if let Some(name) = path.file_name().and_then(|n| n.to_str())
                    && !name.starts_with("seg_")
                {
                    continue;
                }
                let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                entries.push((path, mtime));
            }

            let max_segments =
                self.replay_secs.max(1).div_ceil(REPLAY_SEGMENT_TIME_SECS) as usize + 1;
            let ordered = order_replay_segments(&entries, max_segments);
            if ordered.is_empty() {
                return Err(ClipForgeError::CaptureFailed(
                    "replay buffer has no recorded segments yet".to_string(),
                ));
            }

            // Copy segments into an isolated snapshot dir so FFmpeg's ring-wrap cannot overwrite
            // a segment while concat is reading it.
            let snap_dir = tempfile::Builder::new()
                .prefix("clipforge-replay-snap-")
                .tempdir()?;
            let mut snapped = Vec::with_capacity(ordered.len());
            for (idx, src) in ordered.iter().enumerate() {
                let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
                let dst = snap_dir.path().join(format!("part_{idx:03}.{ext}"));
                if tokio::fs::copy(src, &dst).await.is_ok() {
                    snapped.push(dst);
                }
            }

            if snapped.is_empty() {
                return Err(ClipForgeError::CaptureFailed(
                    "failed to snapshot replay segments".to_string(),
                ));
            }

            let ext = container_extension(self.container);
            let stamp = Utc::now().format("%Y-%m-%d_%H-%M-%S_%3f");
            let output_path = output_dir.join(format!("Replay_{stamp}.{ext}"));

            // Try concatenating all snapped segments; if the newest segment was still open/unfinalized,
            // fall back to all completed segments (snapped[..len-1]).
            let manifest_path = snap_dir.path().join("manifest.txt");
            let manifest = build_concat_manifest(&snapped);
            tokio::fs::write(&manifest_path, manifest).await?;

            let args = build_concat_ffmpeg_args(&manifest_path, &output_path, self.container);
            let mut cmd = Command::new(&ffmpeg);
            cmd.args(&args).creation_flags(CREATE_NO_WINDOW);
            let out = cmd.output().await?;

            if out.status.success() {
                return Ok(output_path);
            }

            if snapped.len() > 1 {
                let completed = &snapped[..snapped.len() - 1];
                let fallback_manifest = build_concat_manifest(completed);
                tokio::fs::write(&manifest_path, fallback_manifest).await?;
                let mut retry_cmd = Command::new(&ffmpeg);
                retry_cmd.args(&args).creation_flags(CREATE_NO_WINDOW);
                let retry_out = retry_cmd.output().await?;
                if retry_out.status.success() {
                    return Ok(output_path);
                }
            }

            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            Err(ClipForgeError::CaptureFailed(format!(
                "ffmpeg replay concat failed (exit {:?}): {stderr}",
                out.status.code()
            )))
        }

        /// Stops the rolling FFmpeg session and cleans up the temporary segment directory.
        pub async fn stop(self) -> Result<()> {
            let res = self.encoder.stop().await;
            drop(self.temp_dir);
            res
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn compute_replay_ring_slots_is_strictly_bounded() {
        assert_eq!(compute_replay_ring_slots(60), 32);
        assert_eq!(compute_replay_ring_slots(5), 5);
        assert_eq!(compute_replay_ring_slots(2), 3);
        assert_eq!(compute_replay_ring_slots(1), 3);
        assert_eq!(compute_replay_ring_slots(0), 3);
    }

    #[test]
    fn order_replay_segments_sorts_chronologically_and_bounds_count() {
        let t0 = SystemTime::UNIX_EPOCH;
        let entries = vec![
            (PathBuf::from("seg_002.mp4"), t0 + Duration::from_secs(4)),
            (PathBuf::from("seg_000.mp4"), t0 + Duration::from_secs(0)),
            (PathBuf::from("seg_003.mp4"), t0 + Duration::from_secs(6)),
            (PathBuf::from("seg_001.mp4"), t0 + Duration::from_secs(2)),
        ];
        let ordered = order_replay_segments(&entries, 3);
        assert_eq!(
            ordered,
            vec![
                PathBuf::from("seg_001.mp4"),
                PathBuf::from("seg_002.mp4"),
                PathBuf::from("seg_003.mp4"),
            ]
        );
    }

    #[test]
    fn build_concat_manifest_escapes_single_quotes() {
        let segs = vec![
            PathBuf::from("/tmp/it's/seg_000.mp4"),
            PathBuf::from("/tmp/clips/seg_001.mp4"),
        ];
        let manifest = build_concat_manifest(&segs);
        assert_eq!(
            manifest,
            "file '/tmp/it'\\''s/seg_000.mp4'\nfile '/tmp/clips/seg_001.mp4'\n"
        );
    }

    #[test]
    fn build_concat_ffmpeg_args_includes_copy_and_faststart_for_mp4() {
        let args: Vec<String> = build_concat_ffmpeg_args(
            Path::new("manifest.txt"),
            Path::new("out.mp4"),
            ContainerFormat::Mp4,
        )
        .into_iter()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();

        assert!(args.windows(2).any(|w| w == ["-f", "concat"]));
        assert!(args.windows(2).any(|w| w == ["-safe", "0"]));
        assert!(args.windows(2).any(|w| w == ["-c", "copy"]));
        assert!(args.windows(2).any(|w| w == ["-movflags", "+faststart"]));
        assert_eq!(args.last().map(String::as_str), Some("out.mp4"));
    }
}
