use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use clipforge_common::{
    AudioTrackInfo, ClipForgeError, MediaProbeResult, MediaProcessor, Result, TrimRequest,
    VideoCodec,
};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{Notify, Semaphore};
use tracing::{debug, warn};
use uuid::Uuid;

const DEFAULT_THUMBNAIL_CONCURRENCY: usize = 4;

/// Cancellation token for [`FfmpegMediaProcessor::trim_with_progress`].
#[derive(Clone, Debug, Default)]
pub struct ExportCancelToken {
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl ExportCancelToken {
    /// Creates a new uncancelled export token.
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation of any in-flight export using this token.
    pub fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::SeqCst) {
            self.notify.notify_waiters();
        }
    }

    /// Returns `true` if cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub(crate) async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.notify.notified().await;
    }
}

/// Encoding quality/speed profile for precise (re-encoded) trims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrimQuality {
    /// Constant Rate Factor (`0..=51` for H.264/HEVC, lower = higher quality; default `20`).
    pub crf: u8,
    /// Encoder speed preset (e.g. `"veryfast"`, `"faster"`, `"fast"`; default `"veryfast"`).
    pub preset: &'static str,
}

impl Default for TrimQuality {
    fn default() -> Self {
        Self {
            crf: 20,
            preset: "veryfast",
        }
    }
}

/// Removes a temporary file on drop unless disarmed after atomic rename.
struct TempFileGuard(Option<PathBuf>);

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self(Some(path))
    }

    fn disarm(&mut self) {
        self.0 = None;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// `MediaProcessor` implementation backed by `ffmpeg` and `ffprobe` subprocesses.
#[derive(Debug, Clone)]
pub struct FfmpegMediaProcessor {
    ffmpeg_bin: PathBuf,
    ffprobe_bin: PathBuf,
    thumbnail_semaphore: Arc<Semaphore>,
    trim_quality: TrimQuality,
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
            trim_quality: TrimQuality::default(),
        }
    }

    /// Sets the default [`TrimQuality`] used for precise (re-encoded) trims.
    pub fn with_trim_quality(mut self, quality: TrimQuality) -> Self {
        self.trim_quality = quality;
        self
    }

    /// Returns the number of currently available thumbnail concurrency permits.
    pub fn available_thumbnail_permits(&self) -> usize {
        self.thumbnail_semaphore.available_permits()
    }

    /// Trims/exports a clip according to `req` while reporting progress (`0.0..=1.0`),
    /// supporting cancellation via `cancel`, and validating the output file with `ffprobe`.
    pub async fn trim_with_progress<F>(
        &self,
        req: &TrimRequest,
        cancel: Option<&ExportCancelToken>,
        on_progress: F,
    ) -> Result<PathBuf>
    where
        F: FnMut(f32) + Send,
    {
        self.trim_with_quality(req, self.trim_quality, cancel, on_progress)
            .await
    }

    /// Trims/exports a clip according to `req` using an explicit [`TrimQuality`] profile,
    /// reporting progress (`0.0..=1.0`), supporting cancellation via `cancel`, and validating
    /// the output file with `ffprobe`.
    pub async fn trim_with_quality<F>(
        &self,
        req: &TrimRequest,
        quality: TrimQuality,
        cancel: Option<&ExportCancelToken>,
        mut on_progress: F,
    ) -> Result<PathBuf>
    where
        F: FnMut(f32) + Send,
    {
        req.validate()?;
        if cancel.is_some_and(ExportCancelToken::is_cancelled) {
            return Err(ClipForgeError::MediaFailed("export cancelled".into()));
        }

        let canonical_in = validate_input_path(&req.source_path).await?;
        let ext = validate_video_extension(&req.output_path)?;
        let resolved_out = prepare_output_path(&req.output_path).await?;
        let temp_out = make_temp_sibling(&resolved_out, &ext);
        let mut temp_guard = TempFileGuard::new(temp_out.clone());

        let total_us = req.end_ms.saturating_sub(req.start_ms).saturating_mul(1000);
        let args = build_trim_args(&canonical_in, &temp_out, req, &ext, quality);

        debug!(
            source = %canonical_in.display(),
            output = %resolved_out.display(),
            start_ms = req.start_ms,
            end_ms = req.end_ms,
            reencode = req.reencode,
            crf = quality.crf,
            preset = quality.preset,
            "trimming media with progress"
        );

        let mut cmd = Command::new(&self.ffmpeg_bin);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                return Err(ClipForgeError::MediaFailed(format!(
                    "failed to spawn ffmpeg ({}): {e}",
                    self.ffmpeg_bin.display()
                )));
            }
        };

        let stdout = child.stdout.take().ok_or_else(|| {
            ClipForgeError::MediaFailed("failed to capture ffmpeg progress stdout".into())
        })?;
        let stderr_pipe = child.stderr.take();
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(mut err) = stderr_pipe {
                let _ = err.read_to_end(&mut buf).await;
            }
            buf
        });

        let mut lines = BufReader::new(stdout).lines();
        on_progress(0.0);

        loop {
            let next_line = if let Some(tok) = cancel {
                tokio::select! {
                    biased;
                    _ = tok.cancelled() => {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                        stderr_task.abort();
                        return Err(ClipForgeError::MediaFailed("export cancelled".into()));
                    }
                    res = lines.next_line() => res,
                }
            } else {
                lines.next_line().await
            };

            match next_line {
                Ok(Some(line)) => {
                    if let Some(out_us) = parse_ffmpeg_progress_us(&line)
                        && total_us > 0
                    {
                        let p = (out_us as f32 / total_us as f32).clamp(0.0, 0.99);
                        on_progress(p);
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    warn!(error = %e, "error reading ffmpeg progress pipe");
                    break;
                }
            }
        }

        let status = if let Some(tok) = cancel {
            tokio::select! {
                biased;
                _ = tok.cancelled() => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    stderr_task.abort();
                    return Err(ClipForgeError::MediaFailed("export cancelled".into()));
                }
                res = child.wait() => res,
            }
        } else {
            child.wait().await
        };

        let status = match status {
            Ok(s) => s,
            Err(e) => {
                stderr_task.abort();
                return Err(ClipForgeError::Io(e));
            }
        };

        let stderr_bytes = stderr_task.await.unwrap_or_default();
        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr_bytes);
            return Err(ClipForgeError::MediaFailed(format!(
                "ffmpeg trim failed with status {}: {}",
                status,
                stderr.trim()
            )));
        }

        // Validate non-empty output and valid container/streams via ffprobe before atomic rename.
        let file_len = tokio::fs::metadata(&temp_out)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        if file_len == 0 {
            return Err(ClipForgeError::MediaFailed(
                "ffmpeg trim produced an empty output file".into(),
            ));
        }

        if let Err(e) = self.probe(&temp_out).await {
            return Err(ClipForgeError::MediaFailed(format!(
                "exported output failed ffprobe validation: {e}"
            )));
        }

        tokio::fs::rename(&temp_out, &resolved_out)
            .await
            .map_err(ClipForgeError::Io)?;
        temp_guard.disarm();

        on_progress(1.0);
        Ok(req.output_path.clone())
    }
}

#[async_trait]
impl MediaProcessor for FfmpegMediaProcessor {
    async fn probe(&self, path: &Path) -> Result<MediaProbeResult> {
        let canonical = validate_input_path(path).await?;
        let file_size = tokio::fs::metadata(&canonical).await?.len();

        let mut cmd = Command::new(&self.ffprobe_bin);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000);
        let output = cmd
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
        self.trim_with_progress(req, None, |_| {}).await
    }
}

/// Builds `ffmpeg` CLI arguments for fast (stream-copy) or precise (re-encoded) trimming.
///
/// Both modes place `-ss` before `-i` for fast input-side keyframe seeking, avoiding
/// full-file decoding on multi-hour recordings, and use `-t <duration>` after `-i`.
pub fn build_trim_args(
    canonical_in: &Path,
    temp_out: &Path,
    req: &TrimRequest,
    container_ext: &str,
    quality: TrimQuality,
) -> Vec<OsString> {
    let start_ts = format_timestamp_ms(req.start_ms);
    let dur_ts = format_timestamp_ms(req.end_ms.saturating_sub(req.start_ms));

    let mut args: Vec<OsString> = vec![
        "-y".into(),
        "-v".into(),
        "error".into(),
        "-nostats".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-ss".into(),
        start_ts.into(),
    ];

    if req.reencode {
        args.push("-accurate_seek".into());
    }

    args.extend([
        "-i".into(),
        canonical_in.as_os_str().to_os_string(),
        "-t".into(),
        dur_ts.into(),
        "-map".into(),
        "0:v?".into(),
        "-map".into(),
        "0:a?".into(),
    ]);

    if req.reencode {
        let vcodec = ffmpeg_video_codec(req.video_codec);
        let acodec = ffmpeg_audio_codec(container_ext);
        args.extend(["-c:v".into(), vcodec.into()]);
        match vcodec {
            "libx264" | "libx265" => {
                args.extend([
                    "-preset".into(),
                    quality.preset.into(),
                    "-crf".into(),
                    quality.crf.min(51).to_string().into(),
                    "-pix_fmt".into(),
                    "yuv420p".into(),
                ]);
            }
            "libsvtav1" => {
                args.extend([
                    "-preset".into(),
                    "8".into(),
                    "-crf".into(),
                    quality.crf.min(63).to_string().into(),
                    "-pix_fmt".into(),
                    "yuv420p".into(),
                ]);
            }
            "libvpx-vp9" => {
                args.extend([
                    "-deadline".into(),
                    "realtime".into(),
                    "-cpu-used".into(),
                    "4".into(),
                    "-crf".into(),
                    quality.crf.min(63).to_string().into(),
                    "-b:v".into(),
                    "0".into(),
                ]);
            }
            "libvpx" => {
                args.extend([
                    "-deadline".into(),
                    "realtime".into(),
                    "-cpu-used".into(),
                    "4".into(),
                    "-b:v".into(),
                    "2M".into(),
                ]);
            }
            _ => {}
        }
        let abitrate = if acodec == "libopus" { "160k" } else { "192k" };
        args.extend(["-c:a".into(), acodec.into(), "-b:a".into(), abitrate.into()]);
    } else {
        args.extend([
            "-c".into(),
            "copy".into(),
            "-avoid_negative_ts".into(),
            "make_zero".into(),
        ]);
    }

    if container_ext == "mp4" || container_ext == "m4v" {
        args.extend(["-movflags".into(), "+faststart".into()]);
    }

    args.push(temp_out.as_os_str().to_os_string());
    args
}

fn parse_ffmpeg_progress_us(line: &str) -> Option<u64> {
    let trimmed = line.trim();
    if let Some(val) = trimmed.strip_prefix("out_time_us=") {
        return val.trim().parse::<i64>().ok().map(|v| v.max(0) as u64);
    }
    if let Some(val) = trimmed.strip_prefix("out_time_ms=") {
        // In ffmpeg -progress output, out_time_ms is emitted in microseconds.
        return val.trim().parse::<i64>().ok().map(|v| v.max(0) as u64);
    }
    None
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
    let mut cmd = Command::new(ffmpeg_bin);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let result = cmd
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

    #[test]
    fn build_trim_args_fast_trim_places_ss_before_i_and_uses_copy() {
        let req = TrimRequest {
            source_path: PathBuf::from("/clips/source.mp4"),
            output_path: PathBuf::from("/clips/out.mp4"),
            start_ms: 5_400_000,
            end_ms: 5_415_000,
            reencode: false,
            video_codec: None,
        };
        let args: Vec<String> = build_trim_args(
            Path::new("/clips/source.mp4"),
            Path::new("/clips/.tmp_out.mp4"),
            &req,
            "mp4",
            TrimQuality::default(),
        )
        .into_iter()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();

        let ss_pos = args.iter().position(|a| a == "-ss").unwrap();
        let i_pos = args.iter().position(|a| a == "-i").unwrap();
        let t_pos = args.iter().position(|a| a == "-t").unwrap();
        assert!(ss_pos < i_pos, "-ss must be before -i: {args:?}");
        assert!(i_pos < t_pos, "-t must be after -i: {args:?}");
        assert_eq!(args[ss_pos + 1], "5400.000");
        assert_eq!(args[t_pos + 1], "15.000");
        assert!(args.windows(2).any(|w| w == ["-c", "copy"]));
        assert!(args.windows(2).any(|w| w == ["-map", "0:v?"]));
        assert!(args.windows(2).any(|w| w == ["-map", "0:a?"]));
        assert!(
            args.windows(2)
                .any(|w| w == ["-avoid_negative_ts", "make_zero"])
        );
        assert!(args.windows(2).any(|w| w == ["-movflags", "+faststart"]));
    }

    #[test]
    fn build_trim_args_precise_trim_uses_veryfast_crf_and_accurate_seek() {
        let req = TrimRequest {
            source_path: PathBuf::from("/clips/source.mp4"),
            output_path: PathBuf::from("/clips/out.mp4"),
            start_ms: 5_401_250,
            end_ms: 5_416_250,
            reencode: true,
            video_codec: Some(VideoCodec::H264),
        };
        let args: Vec<String> = build_trim_args(
            Path::new("/clips/source.mp4"),
            Path::new("/clips/.tmp_out.mp4"),
            &req,
            "mp4",
            TrimQuality {
                crf: 18,
                preset: "veryfast",
            },
        )
        .into_iter()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();

        let ss_pos = args.iter().position(|a| a == "-ss").unwrap();
        let acc_pos = args.iter().position(|a| a == "-accurate_seek").unwrap();
        let i_pos = args.iter().position(|a| a == "-i").unwrap();
        let t_pos = args.iter().position(|a| a == "-t").unwrap();
        assert!(ss_pos < i_pos);
        assert!(acc_pos < i_pos);
        assert!(i_pos < t_pos);
        assert_eq!(args[ss_pos + 1], "5401.250");
        assert_eq!(args[t_pos + 1], "15.000");
        assert!(args.windows(2).any(|w| w == ["-c:v", "libx264"]));
        assert!(args.windows(2).any(|w| w == ["-preset", "veryfast"]));
        assert!(args.windows(2).any(|w| w == ["-crf", "18"]));
        assert!(args.windows(2).any(|w| w == ["-c:a", "aac"]));
        assert!(!args.iter().any(|a| a == "-avoid_negative_ts"));
    }

    #[test]
    fn build_trim_args_preserves_spaces_and_windows_paths() {
        let win_in = Path::new(r"C:\Users\Player One\Videos\My Clip.mp4");
        let win_out = Path::new(r"C:\Users\Player One\Videos\.tmp out.mp4");
        let req = TrimRequest {
            source_path: win_in.to_path_buf(),
            output_path: PathBuf::from(r"C:\Users\Player One\Videos\My Clip (1).mp4"),
            start_ms: 1000,
            end_ms: 6000,
            reencode: false,
            video_codec: None,
        };
        let args = build_trim_args(win_in, win_out, &req, "mp4", TrimQuality::default());
        assert!(args.iter().any(|a| a == win_in.as_os_str()));
        assert_eq!(args.last().unwrap(), win_out.as_os_str());
    }
}
