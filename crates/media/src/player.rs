//! Off-UI-thread H.264/video decoding, playback control, and timeline thumbnail extraction.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use clipforge_common::{ClipForgeError, MediaProbeResult, MediaProcessor, Result};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, ChildStdout, Command};
use tokio::sync::{Semaphore, mpsc};
use tokio::time::Instant;
use tracing::{debug, warn};

use crate::processor::{FfmpegMediaProcessor, format_timestamp_ms, validate_input_path};

/// Default maximum frame dimension (matches Rune's 1024x1024 image atlas fast path).
pub const DEFAULT_MAX_DECODE_DIM: u32 = 1024;

const DEFAULT_FPS: f64 = 30.0;
const MIN_FPS: f64 = 1.0;
const MAX_FPS: f64 = 240.0;
const THUMBNAIL_EXTRACT_CONCURRENCY: usize = 4;

/// A single decoded video frame in tightly packed sRGB RGBA8 format (`width * height * 4` bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedFrame {
    pub width: u32,
    pub height: u32,
    pub pts_ms: u64,
    pub data: Vec<u8>,
}

/// Video stream metadata for playback and aspect-ratio preservation.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoMetadata {
    pub width: u32,
    pub height: u32,
    pub duration_ms: u64,
    pub fps: f64,
    pub aspect_ratio: f64,
    pub has_audio: bool,
}

/// Playback lifecycle state emitted by [`VideoPlayerHandle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Loading,
    Playing,
    Paused,
    Ended,
}

/// Events emitted by the off-UI-thread video decoder.
#[derive(Debug, Clone)]
pub enum PlayerEvent {
    Metadata(VideoMetadata),
    Frame(DecodedFrame),
    StateChanged(PlaybackState),
    Position(u64),
    Error(String),
}

#[derive(Debug)]
enum PlayerCommand {
    Open {
        path: PathBuf,
        trim_range: Option<(u64, u64)>,
        max_dim: Option<u32>,
    },
    Play,
    Pause,
    Seek(u64),
    Restart,
    SetTrimRange(Option<(u64, u64)>),
    Close,
}

/// Handle to an off-UI-thread `ffmpeg` video decoder session.
///
/// Dropping all clones of this handle closes the command channel and terminates
/// any active `ffmpeg` child process.
#[derive(Clone, Debug)]
pub struct VideoPlayerHandle {
    cmd_tx: mpsc::UnboundedSender<PlayerCommand>,
}

impl VideoPlayerHandle {
    /// Spawns a new background video decoder worker using `ffmpeg` and `ffprobe` from `PATH`.
    pub fn spawn() -> (Self, mpsc::UnboundedReceiver<PlayerEvent>) {
        Self::spawn_with_binaries("ffmpeg", "ffprobe")
    }

    /// Spawns a new background video decoder worker with explicit `ffmpeg` and `ffprobe` paths.
    pub fn spawn_with_binaries(
        ffmpeg_bin: impl Into<PathBuf>,
        ffprobe_bin: impl Into<PathBuf>,
    ) -> (Self, mpsc::UnboundedReceiver<PlayerEvent>) {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let ffmpeg_bin = ffmpeg_bin.into();
        let ffprobe_bin = ffprobe_bin.into();

        tokio::spawn(async move {
            let mut worker = PlayerWorker::new(ffmpeg_bin, ffprobe_bin, event_tx);
            worker.run(cmd_rx).await;
        });

        (Self { cmd_tx }, event_rx)
    }

    /// Opens a video file, probes its metadata, and decodes the initial frame at trim start (or 0ms).
    ///
    /// `max_dim` defaults to `Some(1024)` when `None` is not desired; pass `Some(1024)` to fit
    /// Rune's image atlas or `None` to decode at native resolution.
    pub fn open(
        &self,
        path: impl Into<PathBuf>,
        trim_range: Option<(u64, u64)>,
        max_dim: Option<u32>,
    ) {
        let _ = self.cmd_tx.send(PlayerCommand::Open {
            path: path.into(),
            trim_range,
            max_dim,
        });
    }

    /// Starts or resumes playback from the current position (or restarts from trim start if `Ended`).
    pub fn play(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::Play);
    }

    /// Pauses playback and holds the current position.
    pub fn pause(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::Pause);
    }

    /// Seeks to `ms` (clamped to the active trim range) and emits a decoded frame immediately.
    pub fn seek(&self, ms: u64) {
        let _ = self.cmd_tx.send(PlayerCommand::Seek(ms));
    }

    /// Seeks to the active trim start (or 0ms) and begins playing.
    pub fn restart(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::Restart);
    }

    /// Updates the active trim range `[start_ms, end_ms]` (or clears it with `None`).
    pub fn set_trim_range(&self, trim_range: Option<(u64, u64)>) {
        let _ = self.cmd_tx.send(PlayerCommand::SetTrimRange(trim_range));
    }

    /// Stops playback, terminates any running decoder subprocess, and releases clip state.
    pub fn close(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::Close);
    }
}

/// Extracts `count` evenly-spaced RGBA8 thumbnails across the video duration for timeline scrubbing.
pub async fn extract_timeline_thumbnails(
    path: &Path,
    count: usize,
    max_width: u32,
    max_height: u32,
) -> Result<Vec<DecodedFrame>> {
    extract_timeline_thumbnails_with_binaries(
        "ffmpeg", "ffprobe", path, count, max_width, max_height,
    )
    .await
}

/// Extracts `count` evenly-spaced RGBA8 thumbnails using explicit `ffmpeg` and `ffprobe` binaries.
pub async fn extract_timeline_thumbnails_with_binaries(
    ffmpeg_bin: impl AsRef<Path>,
    ffprobe_bin: impl AsRef<Path>,
    path: &Path,
    count: usize,
    max_width: u32,
    max_height: u32,
) -> Result<Vec<DecodedFrame>> {
    if count == 0 {
        return Ok(Vec::new());
    }
    if max_width < 2 || max_height < 2 {
        return Err(ClipForgeError::InvalidInput(
            "thumbnail max_width and max_height must be at least 2".into(),
        ));
    }

    let canonical = validate_input_path(path).await?;
    let processor = FfmpegMediaProcessor::with_binaries(
        ffmpeg_bin.as_ref().to_path_buf(),
        ffprobe_bin.as_ref().to_path_buf(),
        THUMBNAIL_EXTRACT_CONCURRENCY,
    );
    let probe = processor.probe(&canonical).await?;
    let meta = metadata_from_probe(&probe)?;
    let (out_w, out_h) = fit_within_dimensions(meta.width, meta.height, max_width, max_height)?;

    let timestamps = compute_thumbnail_timestamps(meta.duration_ms, count);
    let semaphore = Arc::new(Semaphore::new(THUMBNAIL_EXTRACT_CONCURRENCY));
    let ffmpeg_path = Arc::new(ffmpeg_bin.as_ref().to_path_buf());
    let canonical_arc = Arc::new(canonical);

    let mut handles = Vec::with_capacity(timestamps.len());
    for (idx, ts_ms) in timestamps.into_iter().enumerate() {
        let sem = Arc::clone(&semaphore);
        let ff = Arc::clone(&ffmpeg_path);
        let vid = Arc::clone(&canonical_arc);
        handles.push(tokio::spawn(async move {
            let _permit = sem
                .acquire()
                .await
                .map_err(|_| ClipForgeError::MediaFailed("thumbnail semaphore closed".into()))?;
            let frame = decode_single_frame(&ff, &vid, ts_ms, out_w, out_h).await?;
            Ok::<(usize, DecodedFrame), ClipForgeError>((idx, frame))
        }));
    }

    let mut indexed = Vec::with_capacity(handles.len());
    for handle in handles {
        let res = handle.await.map_err(|e| {
            ClipForgeError::MediaFailed(format!("thumbnail decode task panicked: {e}"))
        })??;
        indexed.push(res);
    }
    indexed.sort_by_key(|(idx, _)| *idx);
    Ok(indexed.into_iter().map(|(_, frame)| frame).collect())
}

struct OpenedClip {
    path: PathBuf,
    metadata: VideoMetadata,
    out_width: u32,
    out_height: u32,
    trim_start_ms: u64,
    trim_end_ms: u64,
    position_ms: u64,
}

struct ActiveStream {
    child: Child,
    stdout: ChildStdout,
    start_ms: u64,
    trim_end_ms: u64,
    fps: f64,
    out_width: u32,
    out_height: u32,
    frame_bytes: usize,
    frame_index: u64,
    epoch: Instant,
}

impl ActiveStream {
    async fn stop(mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

struct PlayerWorker {
    ffmpeg_bin: PathBuf,
    processor: FfmpegMediaProcessor,
    event_tx: mpsc::UnboundedSender<PlayerEvent>,
    clip: Option<OpenedClip>,
    status: PlaybackState,
    stream: Option<ActiveStream>,
}

impl PlayerWorker {
    fn new(
        ffmpeg_bin: PathBuf,
        ffprobe_bin: PathBuf,
        event_tx: mpsc::UnboundedSender<PlayerEvent>,
    ) -> Self {
        let processor = FfmpegMediaProcessor::with_binaries(
            ffmpeg_bin.clone(),
            ffprobe_bin,
            THUMBNAIL_EXTRACT_CONCURRENCY,
        );
        Self {
            ffmpeg_bin,
            processor,
            event_tx,
            clip: None,
            status: PlaybackState::Stopped,
            stream: None,
        }
    }

    fn emit(&self, event: PlayerEvent) {
        let _ = self.event_tx.send(event);
    }

    fn set_status(&mut self, status: PlaybackState) {
        if self.status != status {
            self.status = status;
            self.emit(PlayerEvent::StateChanged(status));
        }
    }

    async fn stop_stream(&mut self) {
        if let Some(stream) = self.stream.take() {
            stream.stop().await;
        }
    }

    async fn run(&mut self, mut cmd_rx: mpsc::UnboundedReceiver<PlayerCommand>) {
        loop {
            if let Some(stream) = self.stream.as_mut() {
                let frame_bytes = stream.frame_bytes;
                let mut buf = vec![0u8; frame_bytes];
                tokio::select! {
                    biased;
                    maybe_cmd = cmd_rx.recv() => {
                        match maybe_cmd {
                            Some(cmd) => self.handle_command(cmd, &mut cmd_rx).await,
                            None => break,
                        }
                    }
                    read_res = stream.stdout.read_exact(&mut buf) => {
                        match read_res {
                            Ok(_) => {
                                let pts_ms = stream.start_ms
                                    + ((stream.frame_index as f64) * 1000.0 / stream.fps).round() as u64;

                                if pts_ms > stream.trim_end_ms {
                                    let end_ms = stream.trim_end_ms;
                                    self.stop_stream().await;
                                    if let Some(clip) = self.clip.as_mut() {
                                        clip.position_ms = end_ms;
                                    }
                                    self.emit(PlayerEvent::Position(end_ms));
                                    self.set_status(PlaybackState::Ended);
                                    continue;
                                }

                                let deadline = stream.epoch
                                    + Duration::from_secs_f64(stream.frame_index as f64 / stream.fps);
                                let out_w = stream.out_width;
                                let out_h = stream.out_height;
                                stream.frame_index += 1;

                                // Pace frame delivery to match clip FPS, while staying responsive to commands.
                                let interrupted = tokio::select! {
                                    biased;
                                    maybe_cmd = cmd_rx.recv() => {
                                        match maybe_cmd {
                                            Some(cmd) => {
                                                self.handle_command(cmd, &mut cmd_rx).await;
                                                true
                                            }
                                            None => return,
                                        }
                                    }
                                    _ = tokio::time::sleep_until(deadline) => false,
                                };

                                if interrupted {
                                    continue;
                                }

                                if let Some(clip) = self.clip.as_mut() {
                                    clip.position_ms = pts_ms;
                                }
                                self.emit(PlayerEvent::Frame(DecodedFrame {
                                    width: out_w,
                                    height: out_h,
                                    pts_ms,
                                    data: buf,
                                }));
                                self.emit(PlayerEvent::Position(pts_ms));
                            }
                            Err(e) if e.kind() == ErrorKind::UnexpectedEof => {
                                self.stop_stream().await;
                                let pos = self.clip.as_ref().map(|c| c.position_ms).unwrap_or(0);
                                self.emit(PlayerEvent::Position(pos));
                                self.set_status(PlaybackState::Ended);
                            }
                            Err(e) => {
                                self.stop_stream().await;
                                self.emit(PlayerEvent::Error(format!("video decode error: {e}")));
                                self.set_status(PlaybackState::Paused);
                            }
                        }
                    }
                }
            } else {
                match cmd_rx.recv().await {
                    Some(cmd) => self.handle_command(cmd, &mut cmd_rx).await,
                    None => break,
                }
            }
        }

        self.stop_stream().await;
    }

    async fn handle_command(
        &mut self,
        cmd: PlayerCommand,
        cmd_rx: &mut mpsc::UnboundedReceiver<PlayerCommand>,
    ) {
        // Coalesce rapid Seek commands during timeline scrubbing.
        let cmd = match cmd {
            PlayerCommand::Seek(mut latest_ms) => {
                let mut override_cmd = None;
                while let Ok(next) = cmd_rx.try_recv() {
                    match next {
                        PlayerCommand::Seek(ms) => latest_ms = ms,
                        other => {
                            override_cmd = Some(other);
                            break;
                        }
                    }
                }
                if let Some(other) = override_cmd {
                    self.execute_command(PlayerCommand::Seek(latest_ms)).await;
                    other
                } else {
                    PlayerCommand::Seek(latest_ms)
                }
            }
            other => other,
        };

        self.execute_command(cmd).await;
    }

    async fn execute_command(&mut self, cmd: PlayerCommand) {
        match cmd {
            PlayerCommand::Open {
                path,
                trim_range,
                max_dim,
            } => {
                self.open_clip(path, trim_range, max_dim).await;
            }
            PlayerCommand::Play => {
                self.start_playback().await;
            }
            PlayerCommand::Pause => {
                if self.status == PlaybackState::Playing {
                    self.stop_stream().await;
                    self.set_status(PlaybackState::Paused);
                }
            }
            PlayerCommand::Seek(ms) => {
                self.seek_to(ms).await;
            }
            PlayerCommand::Restart => {
                if let Some(clip) = self.clip.as_ref() {
                    let start = clip.trim_start_ms;
                    self.seek_to(start).await;
                    self.start_playback().await;
                }
            }
            PlayerCommand::SetTrimRange(range) => {
                self.update_trim_range(range).await;
            }
            PlayerCommand::Close => {
                self.stop_stream().await;
                self.clip = None;
                self.set_status(PlaybackState::Stopped);
            }
        }
    }

    async fn open_clip(
        &mut self,
        path: PathBuf,
        trim_range: Option<(u64, u64)>,
        max_dim: Option<u32>,
    ) {
        self.stop_stream().await;
        self.clip = None;
        self.set_status(PlaybackState::Loading);

        let canonical = match validate_input_path(&path).await {
            Ok(p) => p,
            Err(e) => {
                self.emit(PlayerEvent::Error(e.to_string()));
                self.set_status(PlaybackState::Stopped);
                return;
            }
        };

        let probe = match self.processor.probe(&canonical).await {
            Ok(p) => p,
            Err(e) => {
                self.emit(PlayerEvent::Error(e.to_string()));
                self.set_status(PlaybackState::Stopped);
                return;
            }
        };

        let metadata = match metadata_from_probe(&probe) {
            Ok(m) => m,
            Err(e) => {
                self.emit(PlayerEvent::Error(e.to_string()));
                self.set_status(PlaybackState::Stopped);
                return;
            }
        };

        let (out_width, out_height) =
            match compute_scaled_dimensions(metadata.width, metadata.height, max_dim) {
                Ok(dims) => dims,
                Err(e) => {
                    self.emit(PlayerEvent::Error(e.to_string()));
                    self.set_status(PlaybackState::Stopped);
                    return;
                }
            };

        let (trim_start_ms, trim_end_ms) = normalize_trim_range(trim_range, metadata.duration_ms);
        self.emit(PlayerEvent::Metadata(metadata.clone()));

        match decode_single_frame(
            &self.ffmpeg_bin,
            &canonical,
            trim_start_ms,
            out_width,
            out_height,
        )
        .await
        {
            Ok(frame) => {
                self.clip = Some(OpenedClip {
                    path: canonical,
                    metadata,
                    out_width,
                    out_height,
                    trim_start_ms,
                    trim_end_ms,
                    position_ms: trim_start_ms,
                });
                self.emit(PlayerEvent::Frame(frame));
                self.emit(PlayerEvent::Position(trim_start_ms));
                self.set_status(PlaybackState::Paused);
            }
            Err(e) => {
                self.emit(PlayerEvent::Error(e.to_string()));
                self.set_status(PlaybackState::Stopped);
            }
        }
    }

    async fn start_playback(&mut self) {
        let Some(clip) = self.clip.as_mut() else {
            return;
        };

        if clip.position_ms >= clip.trim_end_ms {
            clip.position_ms = clip.trim_start_ms;
        }

        if clip.trim_start_ms >= clip.trim_end_ms {
            let pos = clip.trim_start_ms;
            self.emit(PlayerEvent::Position(pos));
            self.set_status(PlaybackState::Ended);
            return;
        }

        let start_ms = clip.position_ms;
        let trim_end_ms = clip.trim_end_ms;
        let path = clip.path.clone();
        let fps = clip.metadata.fps;
        let out_w = clip.out_width;
        let out_h = clip.out_height;

        self.stop_stream().await;

        match spawn_playback_stream(
            &self.ffmpeg_bin,
            &path,
            start_ms,
            trim_end_ms,
            fps,
            out_w,
            out_h,
        ) {
            Ok(stream) => {
                self.stream = Some(stream);
                self.set_status(PlaybackState::Playing);
            }
            Err(e) => {
                self.emit(PlayerEvent::Error(e.to_string()));
                self.set_status(PlaybackState::Paused);
            }
        }
    }

    async fn seek_to(&mut self, requested_ms: u64) {
        let was_playing = self.status == PlaybackState::Playing;
        self.stop_stream().await;

        let Some(clip) = self.clip.as_mut() else {
            return;
        };

        let clamped_ms = requested_ms.clamp(clip.trim_start_ms, clip.trim_end_ms);
        let path = clip.path.clone();
        let out_w = clip.out_width;
        let out_h = clip.out_height;

        match decode_single_frame(&self.ffmpeg_bin, &path, clamped_ms, out_w, out_h).await {
            Ok(mut frame) => {
                frame.pts_ms = clamped_ms;
                if let Some(clip) = self.clip.as_mut() {
                    clip.position_ms = clamped_ms;
                }
                self.emit(PlayerEvent::Frame(frame));
                self.emit(PlayerEvent::Position(clamped_ms));
                if was_playing {
                    self.start_playback().await;
                } else {
                    self.set_status(PlaybackState::Paused);
                }
            }
            Err(e) => {
                self.emit(PlayerEvent::Error(e.to_string()));
                self.set_status(PlaybackState::Paused);
            }
        }
    }

    async fn update_trim_range(&mut self, trim_range: Option<(u64, u64)>) {
        let Some(clip) = self.clip.as_mut() else {
            return;
        };
        let (new_start, new_end) = normalize_trim_range(trim_range, clip.metadata.duration_ms);
        clip.trim_start_ms = new_start;
        clip.trim_end_ms = new_end;
        let current_pos = clip.position_ms;
        let clamped_pos = current_pos.clamp(new_start, new_end);

        if clamped_pos != current_pos {
            self.seek_to(clamped_pos).await;
        } else if self.status == PlaybackState::Playing {
            // Restart stream so the new trim_end_ms bound is enforced.
            self.start_playback().await;
        }
    }
}

fn metadata_from_probe(probe: &MediaProbeResult) -> Result<VideoMetadata> {
    if probe.width == 0 || probe.height == 0 {
        return Err(ClipForgeError::MediaFailed(
            "video stream has zero width or height".into(),
        ));
    }
    let fps = if probe.fps.is_finite() && probe.fps > 0.0 {
        (probe.fps as f64).clamp(MIN_FPS, MAX_FPS)
    } else {
        DEFAULT_FPS
    };
    let aspect_ratio = probe.width as f64 / probe.height as f64;
    Ok(VideoMetadata {
        width: probe.width,
        height: probe.height,
        duration_ms: probe.duration_ms,
        fps,
        aspect_ratio,
        has_audio: !probe.audio_tracks.is_empty(),
    })
}

pub(crate) fn compute_scaled_dimensions(
    src_w: u32,
    src_h: u32,
    max_dim: Option<u32>,
) -> Result<(u32, u32)> {
    if src_w == 0 || src_h == 0 {
        return Err(ClipForgeError::MediaFailed(
            "video stream has zero dimensions".into(),
        ));
    }
    let Some(max_d) = max_dim else {
        return Ok((src_w, src_h));
    };
    if max_d < 2 {
        return Err(ClipForgeError::InvalidInput(
            "max_dim must be at least 2".into(),
        ));
    }
    if src_w <= max_d && src_h <= max_d {
        return Ok((src_w, src_h));
    }
    fit_within_dimensions(src_w, src_h, max_d, max_d)
}

pub(crate) fn fit_within_dimensions(
    src_w: u32,
    src_h: u32,
    max_w: u32,
    max_h: u32,
) -> Result<(u32, u32)> {
    if src_w == 0 || src_h == 0 {
        return Err(ClipForgeError::MediaFailed(
            "video stream has zero dimensions".into(),
        ));
    }
    if max_w < 2 || max_h < 2 {
        return Err(ClipForgeError::InvalidInput(
            "target dimensions must be at least 2".into(),
        ));
    }
    let scale = (max_w as f64 / src_w as f64)
        .min(max_h as f64 / src_h as f64)
        .min(1.0);
    let out_w = (((src_w as f64 * scale).round() as u32).clamp(2, max_w)) & !1;
    let out_h = (((src_h as f64 * scale).round() as u32).clamp(2, max_h)) & !1;
    Ok((out_w.max(2), out_h.max(2)))
}

pub(crate) fn normalize_trim_range(trim_range: Option<(u64, u64)>, duration_ms: u64) -> (u64, u64) {
    match trim_range {
        None => (0, duration_ms),
        Some((start, end)) => {
            let s = start.min(duration_ms);
            let e = end.min(duration_ms).max(s);
            (s, e)
        }
    }
}

pub(crate) fn compute_thumbnail_timestamps(duration_ms: u64, count: usize) -> Vec<u64> {
    if count == 0 {
        return Vec::new();
    }
    if count == 1 || duration_ms == 0 {
        return vec![0; count];
    }
    // Sample evenly within [0, duration_ms * (count-1)/count] so the last thumb avoids exact EOF.
    let usable_ms = duration_ms.saturating_sub(1);
    (0..count)
        .map(|i| (usable_ms.saturating_mul(i as u64)) / (count as u64))
        .collect()
}

async fn decode_single_frame(
    ffmpeg_bin: &Path,
    canonical_in: &Path,
    at_ms: u64,
    out_w: u32,
    out_h: u32,
) -> Result<DecodedFrame> {
    let frame_bytes = (out_w as usize)
        .checked_mul(out_h as usize)
        .and_then(|px| px.checked_mul(4))
        .ok_or_else(|| ClipForgeError::MediaFailed("frame buffer size overflow".into()))?;

    let mut attempts = vec![at_ms];
    if at_ms > 0 {
        attempts.push(at_ms.saturating_sub(250));
        if at_ms > 250 {
            attempts.push(0);
        }
    }
    attempts.dedup();

    let mut last_err = None;
    for ts_ms in attempts {
        let ts = format_timestamp_ms(ts_ms);
        let scale_filter = format!("scale={out_w}:{out_h}:flags=fast_bilinear");
        debug!(
            video = %canonical_in.display(),
            ts = %ts,
            out_w,
            out_h,
            "decoding single RGBA frame"
        );

        let output = Command::new(ffmpeg_bin)
            .arg("-v")
            .arg("error")
            .arg("-ss")
            .arg(&ts)
            .arg("-i")
            .arg(canonical_in)
            .arg("-frames:v")
            .arg("1")
            .arg("-vf")
            .arg(&scale_filter)
            .arg("-f")
            .arg("rawvideo")
            .arg("-pix_fmt")
            .arg("rgba")
            .arg("pipe:1")
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|e| {
                ClipForgeError::MediaFailed(format!(
                    "failed to spawn ffmpeg ({}): {e}",
                    ffmpeg_bin.display()
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            last_err = Some(ClipForgeError::MediaFailed(format!(
                "ffmpeg frame decode failed with status {}: {}",
                output.status,
                stderr.trim()
            )));
            continue;
        }

        if output.stdout.len() >= frame_bytes {
            let mut data = output.stdout;
            data.truncate(frame_bytes);
            return Ok(DecodedFrame {
                width: out_w,
                height: out_h,
                pts_ms: at_ms,
                data,
            });
        }

        warn!(
            video = %canonical_in.display(),
            ts_ms,
            got = output.stdout.len(),
            expected = frame_bytes,
            "ffmpeg produced incomplete frame"
        );
        last_err = Some(ClipForgeError::MediaFailed(format!(
            "ffmpeg produced incomplete frame ({} of {} bytes)",
            output.stdout.len(),
            frame_bytes
        )));
    }

    Err(last_err.unwrap_or_else(|| {
        ClipForgeError::MediaFailed("ffmpeg produced no decoded video frame".into())
    }))
}

fn spawn_playback_stream(
    ffmpeg_bin: &Path,
    canonical_in: &Path,
    start_ms: u64,
    trim_end_ms: u64,
    fps: f64,
    out_w: u32,
    out_h: u32,
) -> Result<ActiveStream> {
    let frame_bytes = (out_w as usize)
        .checked_mul(out_h as usize)
        .and_then(|px| px.checked_mul(4))
        .ok_or_else(|| ClipForgeError::MediaFailed("frame buffer size overflow".into()))?;

    let start_ts = format_timestamp_ms(start_ms);
    let dur_ms = trim_end_ms.saturating_sub(start_ms);
    let dur_ts = format_timestamp_ms(dur_ms);
    let scale_filter = format!("scale={out_w}:{out_h}:flags=fast_bilinear");

    debug!(
        video = %canonical_in.display(),
        start_ms,
        trim_end_ms,
        fps,
        out_w,
        out_h,
        "spawning ffmpeg playback stream"
    );

    let mut child = Command::new(ffmpeg_bin)
        .arg("-v")
        .arg("error")
        .arg("-ss")
        .arg(&start_ts)
        .arg("-i")
        .arg(canonical_in)
        .arg("-t")
        .arg(&dur_ts)
        .arg("-vf")
        .arg(&scale_filter)
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgba")
        .arg("pipe:1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            ClipForgeError::MediaFailed(format!(
                "failed to spawn ffmpeg playback stream ({}): {e}",
                ffmpeg_bin.display()
            ))
        })?;

    let stdout = child.stdout.take().ok_or_else(|| {
        ClipForgeError::MediaFailed("failed to capture ffmpeg stdout pipe".into())
    })?;

    Ok(ActiveStream {
        child,
        stdout,
        start_ms,
        trim_end_ms,
        fps,
        out_width: out_w,
        out_height: out_h,
        frame_bytes,
        frame_index: 0,
        epoch: Instant::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_scaled_dimensions_preserves_aspect_and_even_bounds() {
        assert_eq!(
            compute_scaled_dimensions(1920, 1080, Some(1024)).unwrap(),
            (1024, 576)
        );
        assert_eq!(
            compute_scaled_dimensions(1080, 1920, Some(1024)).unwrap(),
            (576, 1024)
        );
        assert_eq!(
            compute_scaled_dimensions(640, 360, Some(1024)).unwrap(),
            (640, 360)
        );
        assert_eq!(
            compute_scaled_dimensions(1920, 1080, None).unwrap(),
            (1920, 1080)
        );
    }

    #[test]
    fn normalize_trim_range_clamps_and_orders() {
        assert_eq!(normalize_trim_range(None, 5000), (0, 5000));
        assert_eq!(normalize_trim_range(Some((1000, 3000)), 5000), (1000, 3000));
        assert_eq!(normalize_trim_range(Some((4000, 2000)), 5000), (4000, 4000));
        assert_eq!(normalize_trim_range(Some((1000, 9000)), 5000), (1000, 5000));
    }

    #[test]
    fn compute_thumbnail_timestamps_distributes_evenly() {
        assert!(compute_thumbnail_timestamps(3000, 0).is_empty());
        assert_eq!(compute_thumbnail_timestamps(3000, 1), vec![0]);
        assert_eq!(compute_thumbnail_timestamps(3000, 3), vec![0, 999, 1999]);
    }
}
