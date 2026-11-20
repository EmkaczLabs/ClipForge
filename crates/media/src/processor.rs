use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use clipforge_common::{
    AudioTrackInfo, ClipForgeError, MediaProbeResult, MediaProcessor, Result, TrimRequest,
    VideoCodec,
};
use serde::Deserialize;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tracing::{debug, warn};
use uuid::Uuid;

const DEFAULT_THUMBNAIL_CONCURRENCY: usize = 4;

/// `MediaProcessor` implementation backed by `ffmpeg` and `ffprobe` subprocesses.
#[derive(Debug, Clone)]
pub struct FfmpegMediaProcessor {
    ffmpeg_bin: PathBuf,
    ffprobe_bin: PathBuf,
    thumbnail_semaphore: Arc<Semaphore>,
}

impl Default for FfmpegMediaProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl FfmpegMediaProcessor {
    /// Creates a processor using `ffmpeg` and `ffprobe` from `PATH` with default concurrency.
    pub fn new() -> Self {
        Self::with_concurrency(DEFAULT_THUMBNAIL_CONCURRENCY)
    }

    /// Creates a processor with a custom maximum number of concurrent thumbnail jobs.
    pub fn with_concurrency(max_concurrent_thumbnails: usize) -> Self {
        Self::with_binaries("ffmpeg", "ffprobe", max_concurrent_thumbnails)
    }

    /// Creates a processor with explicit binary paths and thumbnail concurrency limit.
    pub fn with_binaries(
        ffmpeg_bin: impl Into<PathBuf>,
        ffprobe_bin: impl Into<PathBuf>,
        max_concurrent_thumbnails: usize,
    ) -> Self {
        let permits = max_concurrent_thumbnails.max(1);
        Self {
            ffmpeg_bin: ffmpeg_bin.into(),
            ffprobe_bin: ffprobe_bin.into(),
            thumbnail_semaphore: Arc::new(Semaphore::new(permits)),
        }
    }

    /// Returns the number of currently available thumbnail concurrency permits.
    pub fn available_thumbnail_permits(&self) -> usize {
        self.thumbnail_semaphore.available_permits()
    }
}

#[async_trait]
impl MediaProcessor for FfmpegMediaProcessor {
    async fn probe(&self, path: &Path) -> Result<MediaProbeResult> {
        let canonical = validate_input_path(path).await?;
        let file_size = tokio::fs::metadata(&canonical).await?.len();

        let output = Command::new(&self.ffprobe_bin)
            .arg("-v")
            .arg("quiet")
            .arg("-print_format")
            .arg("json")
            .arg("-show_format")
            .arg("-show_streams")
            .arg(&canonical)
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|e| {
                ClipForgeError::MediaFailed(format!(
                    "failed to spawn ffprobe ({}): {e}",
                    self.ffprobe_bin.display()
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ClipForgeError::MediaFailed(format!(
                "ffprobe exited with status {}: {}",
                output.status,
                stderr.trim()
            )));
        }

        parse_ffprobe_json(&output.stdout, file_size)
    }

    async fn generate_thumbnail(
        &self,
        video_path: &Path,
        at_ms: u64,
        output_path: &Path,
    ) -> Result<()> {
        let _permit = self
            .thumbnail_semaphore
            .acquire()
            .await
            .map_err(|_| ClipForgeError::MediaFailed("thumbnail semaphore closed".into()))?;

        let canonical_in = validate_input_path(video_path).await?;
        let ext = validate_thumbnail_extension(output_path)?;
        let resolved_out = prepare_output_path(output_path).await?;
        let temp_out = make_temp_sibling(&resolved_out, ext);
        let ts = format_timestamp_ms(at_ms);

        debug!(
            video = %canonical_in.display(),
            at_ms,
            output = %resolved_out.display(),
            "generating thumbnail"
        );

        run_thumbnail_ffmpeg(&self.ffmpeg_bin, &canonical_in, &ts, &temp_out).await?;

        let produced = tokio::fs::metadata(&temp_out)
            .await
            .map(|m| m.len() > 0)
            .unwrap_or(false);

        if !produced && at_ms > 0 {
            let _ = tokio::fs::remove_file(&temp_out).await;
            debug!(
                video = %canonical_in.display(),
                at_ms,
                "thumbnail timestamp produced no frame, retrying at 0ms"
            );
            run_thumbnail_ffmpeg(&self.ffmpeg_bin, &canonical_in, "0.000", &temp_out).await?;
        }

        let produced = tokio::fs::metadata(&temp_out)
            .await
            .map(|m| m.len() > 0)
            .unwrap_or(false);
        if !produced {
            let _ = tokio::fs::remove_file(&temp_out).await;
            return Err(ClipForgeError::MediaFailed(format!(
                "ffmpeg produced no thumbnail frame for {}",
                canonical_in.display()
            )));
        }

        if let Err(e) = tokio::fs::rename(&temp_out, &resolved_out).await {
            let _ = tokio::fs::remove_file(&temp_out).await;
            return Err(ClipForgeError::Io(e));
        }

        Ok(())
    }

    async fn trim(&self, req: &TrimRequest) -> Result<PathBuf> {
        req.validate()?;

        let canonical_in = validate_input_path(&req.source_path).await?;
        let ext = validate_video_extension(&req.output_path)?;
        let resolved_out = prepare_output_path(&req.output_path).await?;
        let temp_out = make_temp_sibling(&resolved_out, &ext);

        let start_ts = format_timestamp_ms(req.start_ms);
        let end_ts = format_timestamp_ms(req.end_ms);

        debug!(
            source = %canonical_in.display(),
            output = %resolved_out.display(),
            start_ms = req.start_ms,
            end_ms = req.end_ms,
            reencode = req.reencode,
            "trimming media"
        );

        let mut cmd = Command::new(&self.ffmpeg_bin);
        cmd.arg("-y")
            .arg("-v")
            .arg("error")
            .arg("-ss")
            .arg(&start_ts)
            .arg("-to")
            .arg(&end_ts)
            .arg("-i")
            .arg(&canonical_in)
            .arg("-map")
            .arg("0:v:0?")
            .arg("-map")
            .arg("0:a?");

        if req.reencode {
            let vcodec = ffmpeg_video_codec(req.video_codec);
            let acodec = ffmpeg_audio_codec(&ext);
            cmd.arg("-c:v").arg(vcodec).arg("-c:a").arg(acodec);
        } else {
            cmd.arg("-c")
                .arg("copy")
                .arg("-avoid_negative_ts")
                .arg("make_zero");
        }

        if ext == "mp4" || ext == "m4v" {
            cmd.arg("-movflags").arg("+faststart");
        }

        cmd.arg(&temp_out).kill_on_drop(true);

        let output = match cmd.output().await {
            Ok(out) => out,
            Err(e) => {
                let _ = tokio::fs::remove_file(&temp_out).await;
                return Err(ClipForgeError::MediaFailed(format!(
                    "failed to spawn ffmpeg ({}): {e}",
                    self.ffmpeg_bin.display()
                )));
            }
        };

        if !output.status.success() {
            let _ = tokio::fs::remove_file(&temp_out).await;
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ClipForgeError::MediaFailed(format!(
                "ffmpeg trim failed with status {}: {}",
                output.status,
                stderr.trim()
            )));
        }

        if let Err(e) = tokio::fs::rename(&temp_out, &resolved_out).await {
            let _ = tokio::fs::remove_file(&temp_out).await;
            return Err(ClipForgeError::Io(e));
        }

        Ok(req.output_path.clone())
    }
}

fn validate_path_chars(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Err(ClipForgeError::InvalidInput("path cannot be empty".into()));
    }
    if path.to_string_lossy().contains('\0') {
        return Err(ClipForgeError::InvalidInput(
            "path contains null byte".into(),
        ));
    }
    Ok(())
}

pub(crate) async fn validate_input_path(path: &Path) -> Result<PathBuf> {
    validate_path_chars(path)?;
    let meta = tokio::fs::metadata(path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ClipForgeError::NotFound(format!("media file not found: {}", path.display()))
        } else {
            ClipForgeError::Io(e)
        }
    })?;

    if !meta.is_file() {
        return Err(ClipForgeError::InvalidInput(format!(
            "media path is not a regular file: {}",
            path.display()
        )));
    }

    tokio::fs::canonicalize(path)
        .await
        .map_err(ClipForgeError::Io)
}

async fn prepare_output_path(path: &Path) -> Result<PathBuf> {
    validate_path_chars(path)?;
    let file_name = path.file_name().ok_or_else(|| {
        ClipForgeError::InvalidInput(format!("output path has no file name: {}", path.display()))
    })?;

    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };

    if let Some(parent) = abs_path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let canonical_parent = tokio::fs::canonicalize(parent).await?;
        Ok(canonical_parent.join(file_name))
    } else {
        Ok(abs_path)
    }
}

fn validate_thumbnail_extension(path: &Path) -> Result<&'static str> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| {
            ClipForgeError::InvalidInput(format!(
                "thumbnail path must have an image extension (.png, .jpg, .jpeg, .webp): {}",
                path.display()
            ))
        })?;

    match ext.as_str() {
        "png" => Ok("png"),
        "jpg" | "jpeg" => Ok("jpg"),
        "webp" => Ok("webp"),
        _ => Err(ClipForgeError::InvalidInput(format!(
            "unsupported thumbnail extension '.{ext}' (expected png, jpg, jpeg, or webp)"
        ))),
    }
}

fn validate_video_extension(path: &Path) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| {
            ClipForgeError::InvalidInput(format!(
                "output video path must have a container extension: {}",
                path.display()
            ))
        })?;

    match ext.as_str() {
        "mp4" | "m4v" | "mkv" | "webm" | "flv" | "mov" => Ok(ext),
        _ => Err(ClipForgeError::InvalidInput(format!(
            "unsupported output container extension '.{ext}'"
        ))),
    }
}

fn make_temp_sibling(target: &Path, ext: &str) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!(".clipforge_tmp_{}.{}", Uuid::new_v4(), ext))
}

async fn run_thumbnail_ffmpeg(
    ffmpeg_bin: &Path,
    canonical_in: &Path,
    ts: &str,
    temp_out: &Path,
) -> Result<()> {
    let result = Command::new(ffmpeg_bin)
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-ss")
        .arg(ts)
        .arg("-i")
        .arg(canonical_in)
        .arg("-frames:v")
        .arg("1")
        .arg("-q:v")
        .arg("2")
        .arg(temp_out)
        .kill_on_drop(true)
        .output()
        .await;

    let output = match result {
        Ok(out) => out,
        Err(e) => {
            let _ = tokio::fs::remove_file(temp_out).await;
            return Err(ClipForgeError::MediaFailed(format!(
                "failed to spawn ffmpeg ({}): {e}",
                ffmpeg_bin.display()
            )));
        }
    };

    if !output.status.success() {
        let _ = tokio::fs::remove_file(temp_out).await;
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ClipForgeError::MediaFailed(format!(
            "ffmpeg thumbnail failed with status {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    Ok(())
}

/// Formats milliseconds as `seconds.millis` (e.g. `1500` -> `"1.500"`).
pub(crate) fn format_timestamp_ms(ms: u64) -> String {
    format!("{}.{:03}", ms / 1000, ms % 1000)
}

fn ffmpeg_video_codec(codec: Option<VideoCodec>) -> &'static str {
    match codec {
        None | Some(VideoCodec::Auto) | Some(VideoCodec::H264) => "libx264",
        Some(VideoCodec::Hevc) => "libx265",
        Some(VideoCodec::Av1) => "libsvtav1",
        Some(VideoCodec::Vp8) => "libvpx",
        Some(VideoCodec::Vp9) => "libvpx-vp9",
    }
}

fn ffmpeg_audio_codec(container_ext: &str) -> &'static str {
    match container_ext {
        "webm" => "libopus",
        _ => "aac",
    }
}

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    format: Option<FfprobeFormat>,
    #[serde(default)]
    streams: Vec<FfprobeStream>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
    size: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    index: Option<u32>,
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    channels: Option<u16>,
    sample_rate: Option<String>,
    duration: Option<String>,
    #[serde(default)]
    tags: HashMap<String, String>,
}

pub(crate) fn parse_ffprobe_json(
    json_bytes: &[u8],
    fallback_size_bytes: u64,
) -> Result<MediaProbeResult> {
    let parsed: FfprobeOutput = serde_json::from_slice(json_bytes).map_err(|e| {
        ClipForgeError::Serialization(format!("failed to parse ffprobe JSON output: {e}"))
    })?;

    let video_stream = parsed
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video"))
        .ok_or_else(|| ClipForgeError::MediaFailed("no video stream found in media file".into()))?;

    let duration_ms = parsed
        .format
        .as_ref()
        .and_then(|f| f.duration.as_deref())
        .and_then(parse_seconds_to_ms)
        .or_else(|| {
            video_stream
                .duration
                .as_deref()
                .and_then(parse_seconds_to_ms)
        })
        .unwrap_or(0);

    let size_bytes = parsed
        .format
        .as_ref()
        .and_then(|f| f.size.as_deref())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(fallback_size_bytes);

    let fps = parse_frame_rate(video_stream.avg_frame_rate.as_deref())
        .filter(|&f| f > 0.0)
        .or_else(|| parse_frame_rate(video_stream.r_frame_rate.as_deref()))
        .unwrap_or(0.0);

    let width = video_stream.width.unwrap_or(0);
    let height = video_stream.height.unwrap_or(0);
    if width == 0 || height == 0 {
        warn!("ffprobe reported zero video dimensions");
    }

    let video_codec = video_stream
        .codec_name
        .clone()
        .unwrap_or_else(|| "unknown".into());

    let audio_tracks = parsed
        .streams
        .iter()
        .filter(|s| s.codec_type.as_deref() == Some("audio"))
        .enumerate()
        .map(|(fallback_idx, s)| {
            let label = s
                .tags
                .get("title")
                .or_else(|| s.tags.get("handler_name"))
                .or_else(|| s.tags.get("language"))
                .cloned();
            AudioTrackInfo {
                index: s.index.unwrap_or(fallback_idx as u32),
                codec: s.codec_name.clone().unwrap_or_else(|| "unknown".into()),
                channels: s.channels.unwrap_or(0),
                sample_rate: s
                    .sample_rate
                    .as_deref()
                    .and_then(|r| r.parse::<u32>().ok())
                    .unwrap_or(0),
                label,
            }
        })
        .collect();

    Ok(MediaProbeResult {
        duration_ms,
        width,
        height,
        fps,
        video_codec,
        size_bytes,
        audio_tracks,
    })
}

fn parse_seconds_to_ms(raw: &str) -> Option<u64> {
    let secs = raw.trim().parse::<f64>().ok()?;
    if secs.is_finite() && secs >= 0.0 {
        Some((secs * 1000.0).round() as u64)
    } else {
        None
    }
}

pub(crate) fn parse_frame_rate(raw: Option<&str>) -> Option<f32> {
    let s = raw?.trim();
    if s.is_empty() {
        return None;
    }
    if let Some((num_s, den_s)) = s.split_once('/') {
        let num = num_s.parse::<f64>().ok()?;
        let den = den_s.parse::<f64>().ok()?;
        if den > 0.0 && num >= 0.0 {
            let val = num / den;
            if val.is_finite() {
                return Some(val as f32);
            }
        }
        None
    } else {
        let val = s.parse::<f32>().ok()?;
        if val.is_finite() && val >= 0.0 {
            Some(val)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_timestamp_ms_formats_correctly() {
        assert_eq!(format_timestamp_ms(0), "0.000");
        assert_eq!(format_timestamp_ms(42), "0.042");
        assert_eq!(format_timestamp_ms(1500), "1.500");
        assert_eq!(format_timestamp_ms(65_007), "65.007");
    }

    #[test]
    fn parse_frame_rate_handles_fractions_and_zero() {
        assert!((parse_frame_rate(Some("60/1")).unwrap() - 60.0).abs() < 0.001);
        assert!((parse_frame_rate(Some("30000/1001")).unwrap() - 29.97).abs() < 0.01);
        assert!((parse_frame_rate(Some("59.94")).unwrap() - 59.94).abs() < 0.01);
        assert!(parse_frame_rate(Some("0/0")).is_none());
        assert!(parse_frame_rate(Some("")).is_none());
        assert!(parse_frame_rate(None).is_none());
    }

    #[test]
    fn parse_ffprobe_json_extracts_streams_and_falls_back() {
        let json = br#"{
            "format": {
                "duration": "12.345",
                "size": "1048576"
            },
            "streams": [
                {
                    "index": 0,
                    "codec_type": "video",
                    "codec_name": "hevc",
                    "width": 1920,
                    "height": 1080,
                    "avg_frame_rate": "0/0",
                    "r_frame_rate": "60/1"
                },
                {
                    "index": 1,
                    "codec_type": "audio",
                    "codec_name": "opus",
                    "channels": 2,
                    "sample_rate": "48000",
                    "tags": { "title": "Game Audio" }
                }
            ]
        }"#;

        let res = parse_ffprobe_json(json, 999).expect("valid ffprobe json");
        assert_eq!(res.duration_ms, 12_345);
        assert_eq!(res.size_bytes, 1_048_576);
        assert_eq!(res.width, 1920);
        assert_eq!(res.height, 1080);
        assert!((res.fps - 60.0).abs() < 0.001);
        assert_eq!(res.video_codec, "hevc");
        assert_eq!(res.audio_tracks.len(), 1);
        assert_eq!(res.audio_tracks[0].codec, "opus");
        assert_eq!(res.audio_tracks[0].channels, 2);
        assert_eq!(res.audio_tracks[0].sample_rate, 48_000);
        assert_eq!(res.audio_tracks[0].label.as_deref(), Some("Game Audio"));
    }

    #[test]
    fn parse_ffprobe_json_rejects_audio_only() {
        let json = br#"{
            "format": { "duration": "3.0" },
            "streams": [{ "index": 0, "codec_type": "audio", "codec_name": "aac" }]
        }"#;
        let err = parse_ffprobe_json(json, 0).unwrap_err();
        assert!(matches!(err, ClipForgeError::MediaFailed(_)));
    }
}
