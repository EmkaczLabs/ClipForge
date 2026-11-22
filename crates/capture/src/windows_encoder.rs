use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clipforge_common::{
    AudioCodec, ClipForgeError, ContainerFormat, EncoderMode, QualityPreset, Result, VideoCodec,
};
use tokio::process::Command;

/// Windows process creation flag that suppresses spawning a console window for child `ffmpeg.exe`.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Target audio sample rate used for WASAPI mixing and FFmpeg `f32le` input.
pub const WINDOWS_AUDIO_SAMPLE_RATE: u32 = 48_000;

/// Target audio channel count (stereo) used for WASAPI mixing and FFmpeg `f32le` input.
pub const WINDOWS_AUDIO_CHANNELS: u16 = 2;

/// Output mode for [`build_windows_ffmpeg_args`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowsOutputMode {
    /// Standard single-file recording (adds `-movflags +faststart` for MP4).
    SingleFile(PathBuf),
    /// Rolling segmented replay buffer (`-f segment -segment_wrap <slots> -reset_timestamps 1`).
    RollingSegment {
        segment_time_secs: u32,
        segment_wrap: u32,
        pattern: PathBuf,
    },
}

/// Parameters for constructing a Windows FFmpeg capture/muxing command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsEncoderParams {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub video_codec: String,
    pub audio_codec: AudioCodec,
    pub quality: QualityPreset,
    pub container: ContainerFormat,
    pub audio_pipe_path: Option<String>,
    pub audio_sample_rate: u32,
    pub audio_channels: u16,
    pub output: WindowsOutputMode,
}

/// Searches the executable directory and `PATH` for `ffmpeg` (`ffmpeg.exe` on Windows).
pub fn find_ffmpeg_binary() -> Option<PathBuf> {
    let bin_name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };

    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    {
        let candidate = exe_dir.join(bin_name);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }

    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(bin_name);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable_file(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(meta) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                meta.is_file() && (meta.permissions().mode() & 0o111 != 0)
            }
            #[cfg(not(unix))]
            {
                meta.is_file()
            }
        }
        Err(_) => false,
    }
}

/// Parses `ffmpeg -hide_banner -encoders` stdout into a list of encoder names.
pub fn parse_ffmpeg_encoders_output(stdout: &str) -> Vec<String> {
    let mut encoders = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim_start();
        // Format: " V..... h264_nvenc           NVIDIA NVENC H.264 encoder"
        // Skip header lines and separator "------"
        if trimmed.len() < 8 || trimmed.starts_with('-') || trimmed.starts_with("Encoders:") {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let Some(flags) = parts.next() else {
            continue;
        };
        if flags.len() != 6 {
            continue;
        }
        let kind = flags.as_bytes()[0];
        if kind != b'V' && kind != b'A' && kind != b'S' {
            continue;
        }
        if let Some(name) = parts.next()
            && name != "="
        {
            encoders.push(name.to_string());
        }
    }
    encoders
}

/// Selects the best Windows FFmpeg video encoder for the requested codec and mode,
/// probing hardware encoders (`*_nvenc`, `*_amf`, `*_qsv`) with software fallback.
pub fn select_windows_video_encoder(
    codec: VideoCodec,
    mode: EncoderMode,
    available_encoders: &[&str],
) -> &'static str {
    let has = |name: &str| available_encoders.contains(&name);

    match mode {
        EncoderMode::Cpu => match codec {
            VideoCodec::Auto | VideoCodec::H264 => "libx264",
            VideoCodec::Hevc => "libx265",
            VideoCodec::Av1 => {
                if has("libsvtav1") {
                    "libsvtav1"
                } else if has("libaom-av1") {
                    "libaom-av1"
                } else {
                    "libsvtav1"
                }
            }
            VideoCodec::Vp8 => "libvpx",
            VideoCodec::Vp9 => "libvpx-vp9",
        },
        EncoderMode::Gpu => match codec {
            VideoCodec::Auto | VideoCodec::H264 => {
                for candidate in ["h264_nvenc", "h264_amf", "h264_qsv"] {
                    if has(candidate) {
                        return candidate;
                    }
                }
                "libx264"
            }
            VideoCodec::Hevc => {
                for candidate in ["hevc_nvenc", "hevc_amf", "hevc_qsv"] {
                    if has(candidate) {
                        return candidate;
                    }
                }
                "libx265"
            }
            VideoCodec::Av1 => {
                for candidate in ["av1_nvenc", "av1_amf", "av1_qsv"] {
                    if has(candidate) {
                        return candidate;
                    }
                }
                if has("libsvtav1") {
                    "libsvtav1"
                } else if has("libaom-av1") {
                    "libaom-av1"
                } else {
                    "libsvtav1"
                }
            }
            VideoCodec::Vp8 => "libvpx",
            VideoCodec::Vp9 => "libvpx-vp9",
        },
    }
}

/// Probes available FFmpeg encoders by invoking `ffmpeg -hide_banner -encoders`.
pub async fn probe_ffmpeg_encoders() -> Result<Vec<String>> {
    let ffmpeg = find_ffmpeg_binary().ok_or_else(|| {
        ClipForgeError::CaptureUnavailable(
            "ffmpeg.exe was not found in the application directory or PATH".to_string(),
        )
    })?;

    let mut cmd = Command::new(&ffmpeg);
    cmd.args(["-hide_banner", "-encoders"]);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let out = cmd.output().await?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(ClipForgeError::CaptureFailed(format!(
            "ffmpeg -encoders failed (exit {:?}): {stderr}",
            out.status.code()
        )));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(parse_ffmpeg_encoders_output(&stdout))
}

fn quality_qp_str(quality: QualityPreset) -> &'static str {
    match quality {
        QualityPreset::Medium => "24",
        QualityPreset::High => "20",
        QualityPreset::VeryHigh => "17",
        QualityPreset::Ultra => "14",
    }
}

fn segment_format_name(container: ContainerFormat) -> &'static str {
    match container {
        ContainerFormat::Mp4 => "mp4",
        ContainerFormat::Mkv => "matroska",
        ContainerFormat::Webm => "webm",
        ContainerFormat::Flv => "flv",
    }
}

/// Builds the FFmpeg CLI argument vector for Windows raw BGRA + `f32le` PCM muxing.
pub fn build_windows_ffmpeg_args(params: &WindowsEncoderParams) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::with_capacity(48);
    let push = |args: &mut Vec<OsString>, s: &str| args.push(OsString::from(s));

    push(&mut args, "-y");
    push(&mut args, "-hide_banner");
    push(&mut args, "-loglevel");
    push(&mut args, "warning");

    // Raw BGRA video stream on stdin (pipe:0)
    push(&mut args, "-f");
    push(&mut args, "rawvideo");
    push(&mut args, "-pixel_format");
    push(&mut args, "bgra");
    push(&mut args, "-video_size");
    push(&mut args, &format!("{}x{}", params.width, params.height));
    push(&mut args, "-framerate");
    push(&mut args, &params.fps.max(1).to_string());
    push(&mut args, "-i");
    push(&mut args, "pipe:0");

    // Optional f32le PCM audio stream on a Windows named pipe
    if let Some(ref audio_pipe) = params.audio_pipe_path {
        push(&mut args, "-f");
        push(&mut args, "f32le");
        push(&mut args, "-ar");
        push(&mut args, &params.audio_sample_rate.to_string());
        push(&mut args, "-ac");
        push(&mut args, &params.audio_channels.to_string());
        push(&mut args, "-i");
        push(&mut args, audio_pipe);
    }

    push(&mut args, "-map");
    push(&mut args, "0:v:0");
    if params.audio_pipe_path.is_some() {
        push(&mut args, "-map");
        push(&mut args, "1:a:0");
    }

    // Video encoder & pixel format
    push(&mut args, "-c:v");
    push(&mut args, &params.video_codec);
    push(&mut args, "-pix_fmt");
    push(&mut args, "yuv420p");

    let gop = params.fps.max(1).saturating_mul(2);
    push(&mut args, "-g");
    push(&mut args, &gop.to_string());

    let qp = quality_qp_str(params.quality);
    if params.video_codec.ends_with("_nvenc") {
        push(&mut args, "-preset");
        push(&mut args, "p4");
        push(&mut args, "-rc");
        push(&mut args, "vbr");
        push(&mut args, "-cq");
        push(&mut args, qp);
    } else if params.video_codec.ends_with("_amf") {
        push(&mut args, "-quality");
        push(&mut args, "balanced");
        push(&mut args, "-rc");
        push(&mut args, "cqp");
        push(&mut args, "-qp_i");
        push(&mut args, qp);
        push(&mut args, "-qp_p");
        push(&mut args, qp);
    } else if params.video_codec.ends_with("_qsv") {
        push(&mut args, "-preset");
        push(&mut args, "medium");
        push(&mut args, "-global_quality");
        push(&mut args, qp);
    } else if params.video_codec == "libsvtav1" {
        push(&mut args, "-preset");
        push(&mut args, "6");
        push(&mut args, "-crf");
        push(&mut args, qp);
    } else {
        push(&mut args, "-preset");
        push(&mut args, "veryfast");
        push(&mut args, "-crf");
        push(&mut args, qp);
    }

    // Audio encoder
    if params.audio_pipe_path.is_some() {
        match params.audio_codec {
            AudioCodec::Aac => {
                push(&mut args, "-c:a");
                push(&mut args, "aac");
                push(&mut args, "-b:a");
                push(&mut args, "192k");
            }
            AudioCodec::Opus => {
                push(&mut args, "-c:a");
                push(&mut args, "libopus");
                push(&mut args, "-b:a");
                push(&mut args, "160k");
            }
            AudioCodec::Flac => {
                push(&mut args, "-c:a");
                push(&mut args, "flac");
            }
        }
    }

    match &params.output {
        WindowsOutputMode::SingleFile(path) => {
            if params.container == ContainerFormat::Mp4 {
                push(&mut args, "-movflags");
                push(&mut args, "+faststart");
            }
            args.push(path.as_os_str().to_os_string());
        }
        WindowsOutputMode::RollingSegment {
            segment_time_secs,
            segment_wrap,
            pattern,
        } => {
            push(&mut args, "-f");
            push(&mut args, "segment");
            push(&mut args, "-segment_time");
            push(&mut args, &(*segment_time_secs).max(1).to_string());
            push(&mut args, "-segment_wrap");
            push(&mut args, &(*segment_wrap).max(1).to_string());
            push(&mut args, "-reset_timestamps");
            push(&mut args, "1");
            push(&mut args, "-segment_format");
            push(&mut args, segment_format_name(params.container));
            args.push(pattern.as_os_str().to_os_string());
        }
    }

    args
}

/// Mixes two interleaved `f32` PCM buffers with clamping to `[-1.0, 1.0]`.
///
/// If one buffer is shorter than the other, the missing tail is treated as silence (`0.0`).
pub fn mix_audio_chunks(loopback: &[f32], mic: &[f32]) -> Vec<f32> {
    let len = loopback.len().max(mic.len());
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let a = loopback.get(i).copied().unwrap_or(0.0);
        let b = mic.get(i).copied().unwrap_or(0.0);
        out.push((a + b).clamp(-1.0, 1.0));
    }
    out
}

/// Normalizes interleaved `f32` PCM samples from `src_channels` to `dst_channels`.
pub fn normalize_channels_interleaved(
    samples: &[f32],
    src_channels: u16,
    dst_channels: u16,
) -> Vec<f32> {
    let src = src_channels.max(1) as usize;
    let dst = dst_channels.max(1) as usize;
    if src == dst {
        return samples.to_vec();
    }
    let frames = samples.len() / src;
    let mut out = Vec::with_capacity(frames * dst);
    for frame in samples.chunks_exact(src) {
        if src == 1 && dst == 2 {
            out.push(frame[0]);
            out.push(frame[0]);
        } else if src == 2 && dst == 1 {
            out.push((frame[0] + frame[1]) * 0.5);
        } else {
            for ch in 0..dst {
                out.push(frame.get(ch).copied().unwrap_or(frame[0]));
            }
        }
    }
    out
}

/// Linearly resamples interleaved `f32` PCM samples from `src_rate` to `dst_rate`.
pub fn resample_linear_interleaved(
    samples: &[f32],
    src_rate: u32,
    dst_rate: u32,
    channels: u16,
) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    if src_rate == dst_rate || src_rate == 0 || dst_rate == 0 || samples.len() < ch {
        return samples.to_vec();
    }
    let src_frames = samples.len() / ch;
    let dst_frames = ((src_frames as u64 * dst_rate as u64) / src_rate as u64).max(1) as usize;
    let mut out = Vec::with_capacity(dst_frames * ch);
    let ratio = src_rate as f64 / dst_rate as f64;
    for i in 0..dst_frames {
        let src_pos = i as f64 * ratio;
        let idx = (src_pos.floor() as usize).min(src_frames - 1);
        let next = (idx + 1).min(src_frames - 1);
        let frac = (src_pos - idx as f64) as f32;
        for c in 0..ch {
            let a = samples[idx * ch + c];
            let b = samples[next * ch + c];
            out.push(a + (b - a) * frac);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Windows-only live FFmpeg encoding session (WGC BGRA + WASAPI f32le)
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
pub mod runtime {
    use std::process::Stdio;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::windows::named_pipe::ServerOptions;
    use tokio::process::{Child, Command};
    use tokio::sync::mpsc;
    use tokio::task::JoinHandle;
    use tracing::{debug, warn};
    use uuid::Uuid;

    use super::{
        CREATE_NO_WINDOW, WINDOWS_AUDIO_CHANNELS, WINDOWS_AUDIO_SAMPLE_RATE, WindowsEncoderParams,
        WindowsOutputMode, build_windows_ffmpeg_args, find_ffmpeg_binary, mix_audio_chunks,
        normalize_channels_interleaved, probe_ffmpeg_encoders, resample_linear_interleaved,
        select_windows_video_encoder,
    };
    use crate::windows_audio::CapturedAudioChunk;
    use crate::windows_wgc::CapturedVideoFrame;
    use clipforge_common::{CaptureConfig, ClipForgeError, Result};

    const FFMPEG_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

    /// Active Windows FFmpeg muxing session consuming WGC video frames and WASAPI audio chunks.
    pub struct WindowsEncoderSession {
        child: Child,
        stop_flag: Arc<AtomicBool>,
        video_task: Option<JoinHandle<()>>,
        audio_task: Option<JoinHandle<()>>,
        stderr_task: Option<JoinHandle<Vec<String>>>,
    }

    impl WindowsEncoderSession {
        /// Probes FFmpeg encoders, spawns `ffmpeg.exe` (`CREATE_NO_WINDOW`), and starts
        /// synchronized video/audio feeder tasks.
        pub async fn start(
            config: &CaptureConfig,
            width: u32,
            height: u32,
            output: WindowsOutputMode,
            video_rx: mpsc::Receiver<CapturedVideoFrame>,
            loopback_rx: Option<mpsc::Receiver<CapturedAudioChunk>>,
            mic_rx: Option<mpsc::Receiver<CapturedAudioChunk>>,
        ) -> Result<Self> {
            let ffmpeg = find_ffmpeg_binary().ok_or_else(|| {
                ClipForgeError::CaptureUnavailable(
                    "ffmpeg.exe was not found in the application directory or PATH".to_string(),
                )
            })?;

            let available = probe_ffmpeg_encoders().await.unwrap_or_default();
            let available_refs: Vec<&str> = available.iter().map(String::as_str).collect();
            let video_codec =
                select_windows_video_encoder(config.video_codec, config.encoder, &available_refs)
                    .to_string();

            let has_audio = loopback_rx.is_some() || mic_rx.is_some();
            let audio_pipe_path = if has_audio {
                Some(format!(r"\\.\pipe\clipforge-audio-{}", Uuid::new_v4()))
            } else {
                None
            };

            let pipe_server = if let Some(ref pipe_name) = audio_pipe_path {
                Some(
                    ServerOptions::new()
                        .first_pipe_instance(true)
                        .create(pipe_name)
                        .map_err(|e| {
                            ClipForgeError::CaptureFailed(format!(
                                "failed to create Windows audio named pipe {pipe_name}: {e}"
                            ))
                        })?,
                )
            } else {
                None
            };

            let params = WindowsEncoderParams {
                width,
                height,
                fps: config.fps.max(1),
                video_codec,
                audio_codec: config.audio_codec,
                quality: config.quality,
                container: config.container,
                audio_pipe_path,
                audio_sample_rate: WINDOWS_AUDIO_SAMPLE_RATE,
                audio_channels: WINDOWS_AUDIO_CHANNELS,
                output,
            };

            let args = build_windows_ffmpeg_args(&params);
            let mut cmd = Command::new(&ffmpeg);
            cmd.args(&args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .creation_flags(CREATE_NO_WINDOW);

            let mut child = cmd.spawn().map_err(|e| {
                ClipForgeError::CaptureFailed(format!("failed to spawn ffmpeg.exe: {e}"))
            })?;

            let stdin = child.stdin.take().ok_or_else(|| {
                ClipForgeError::CaptureFailed("ffmpeg stdin was not piped".to_string())
            })?;
            let stderr = child.stderr.take();

            let stop_flag = Arc::new(AtomicBool::new(false));
            let expected_bytes = (width as usize)
                .saturating_mul(height as usize)
                .saturating_mul(4);
            let fps = config.fps.max(1);

            let video_stop = Arc::clone(&stop_flag);
            let video_task = tokio::spawn(async move {
                run_video_feeder(stdin, video_rx, expected_bytes, fps, video_stop).await;
            });

            let audio_task = pipe_server.map(|server| {
                let audio_stop = Arc::clone(&stop_flag);
                tokio::spawn(async move {
                    run_audio_feeder(server, loopback_rx, mic_rx, audio_stop).await;
                })
            });

            let stderr_task = stderr.map(|err| {
                tokio::spawn(async move {
                    let mut lines = Vec::new();
                    let mut reader = BufReader::new(err).lines();
                    while let Ok(Some(line)) = reader.next_line().await {
                        debug!(target: "clipforge::ffmpeg", "{line}");
                        lines.push(line);
                    }
                    lines
                })
            });

            Ok(Self {
                child,
                stop_flag,
                video_task: Some(video_task),
                audio_task,
                stderr_task,
            })
        }

        /// Checks whether the FFmpeg child has exited unexpectedly.
        pub fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
            self.child.try_wait()
        }

        /// Cleanly stops the feeder tasks (closing `stdin` EOF + audio pipe) and waits
        /// for `ffmpeg.exe` to finalize the output file.
        pub async fn stop(mut self) -> Result<()> {
            self.stop_flag.store(true, Ordering::SeqCst);

            if let Some(vt) = self.video_task.take() {
                let _ = vt.await;
            }
            if let Some(at) = self.audio_task.take() {
                let _ = at.await;
            }

            let wait_res = tokio::time::timeout(FFMPEG_SHUTDOWN_TIMEOUT, self.child.wait()).await;
            let stderr_lines = if let Some(st) = self.stderr_task.take() {
                st.await.unwrap_or_default()
            } else {
                Vec::new()
            };

            match wait_res {
                Ok(Ok(status)) if status.success() => Ok(()),
                Ok(Ok(status)) => {
                    let tail = stderr_lines
                        .into_iter()
                        .rev()
                        .take(5)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>()
                        .join(" | ");
                    Err(ClipForgeError::CaptureFailed(format!(
                        "ffmpeg.exe exited with {status}: {tail}"
                    )))
                }
                Ok(Err(e)) => Err(ClipForgeError::CaptureFailed(format!(
                    "failed waiting on ffmpeg.exe: {e}"
                ))),
                Err(_) => {
                    warn!("ffmpeg.exe did not exit within timeout after stdin EOF; killing");
                    let _ = self.child.kill().await;
                    let _ = self.child.wait().await;
                    Err(ClipForgeError::CaptureFailed(
                        "ffmpeg.exe timed out during finalization".to_string(),
                    ))
                }
            }
        }
    }

    async fn run_video_feeder(
        mut stdin: tokio::process::ChildStdin,
        mut video_rx: mpsc::Receiver<CapturedVideoFrame>,
        expected_bytes: usize,
        fps: u32,
        stop_flag: Arc<AtomicBool>,
    ) {
        let frame_period = Duration::from_secs_f64(1.0 / fps.max(1) as f64);
        let mut ticker = tokio::time::interval(frame_period);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut last_frame: Option<Vec<u8>> = None;

        // Wait for the first real frame before starting CFR pacing so we don't write black frames.
        while !stop_flag.load(Ordering::Relaxed) {
            match video_rx.recv().await {
                Some(frame) if frame.bgra.len() == expected_bytes => {
                    if stdin.write_all(&frame.bgra).await.is_err() {
                        return;
                    }
                    last_frame = Some(frame.bgra);
                    break;
                }
                Some(_) => continue,
                None => {
                    let _ = stdin.shutdown().await;
                    return;
                }
            }
        }

        while !stop_flag.load(Ordering::Relaxed) {
            ticker.tick().await;
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }
            // Drain to the latest available frame so capture stays low-latency.
            let mut got_new = false;
            loop {
                match video_rx.try_recv() {
                    Ok(frame) if frame.bgra.len() == expected_bytes => {
                        last_frame = Some(frame.bgra);
                        got_new = true;
                    }
                    Ok(_) => continue,
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        let _ = stdin.shutdown().await;
                        return;
                    }
                }
            }
            let _ = got_new;
            if let Some(ref buf) = last_frame
                && stdin.write_all(buf).await.is_err()
            {
                break;
            }
        }

        let _ = stdin.shutdown().await;
    }

    async fn run_audio_feeder(
        pipe: tokio::net::windows::named_pipe::NamedPipeServer,
        mut loopback_rx: Option<mpsc::Receiver<CapturedAudioChunk>>,
        mut mic_rx: Option<mpsc::Receiver<CapturedAudioChunk>>,
        stop_flag: Arc<AtomicBool>,
    ) {
        if let Err(e) = pipe.connect().await {
            warn!(error = %e, "ffmpeg did not connect to Windows audio named pipe");
            return;
        }
        let mut pipe = pipe;

        let normalize = |chunk: CapturedAudioChunk| -> Vec<f32> {
            let ch_norm = normalize_channels_interleaved(
                &chunk.samples_f32,
                chunk.channels,
                WINDOWS_AUDIO_CHANNELS,
            );
            resample_linear_interleaved(
                &ch_norm,
                chunk.sample_rate,
                WINDOWS_AUDIO_SAMPLE_RATE,
                WINDOWS_AUDIO_CHANNELS,
            )
        };

        let mut interval = tokio::time::interval(Duration::from_millis(20));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        while !stop_flag.load(Ordering::Relaxed) {
            interval.tick().await;
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            let mut lb_buf = Vec::new();
            if let Some(ref mut rx) = loopback_rx {
                while let Ok(chunk) = rx.try_recv() {
                    lb_buf.extend_from_slice(&normalize(chunk));
                }
            }

            let mut mic_buf = Vec::new();
            if let Some(ref mut rx) = mic_rx {
                while let Ok(chunk) = rx.try_recv() {
                    mic_buf.extend_from_slice(&normalize(chunk));
                }
            }

            if lb_buf.is_empty() && mic_buf.is_empty() {
                continue;
            }

            let mixed = mix_audio_chunks(&lb_buf, &mic_buf);
            let mut bytes = Vec::with_capacity(mixed.len() * 4);
            for sample in mixed {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
            if pipe.write_all(&bytes).await.is_err() {
                break;
            }
        }

        let _ = pipe.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ffmpeg_encoders_extracts_hardware_and_software_encoders() {
        let sample = "\
Encoders:
 V..... = Video
 A..... = Audio
 ------
 V..... libx264              libx264 H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10 (codec h264)
 V....D h264_nvenc           NVIDIA NVENC H.264 encoder (codec h264)
 V..... h264_amf             AMD AMF H.264 Encoder (codec h264)
 V..... h264_qsv             H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10 (Intel Quick Sync Video acceleration) (codec h264)
 A..... aac                  AAC (Advanced Audio Coding)
";
        let encoders = parse_ffmpeg_encoders_output(sample);
        assert_eq!(
            encoders,
            vec!["libx264", "h264_nvenc", "h264_amf", "h264_qsv", "aac"]
        );
    }

    #[test]
    fn select_windows_video_encoder_prefers_nvenc_then_amf_then_qsv_then_libx264() {
        assert_eq!(
            select_windows_video_encoder(
                VideoCodec::Auto,
                EncoderMode::Gpu,
                &["libx264", "h264_qsv", "h264_amf", "h264_nvenc"],
            ),
            "h264_nvenc"
        );
        assert_eq!(
            select_windows_video_encoder(
                VideoCodec::H264,
                EncoderMode::Gpu,
                &["libx264", "h264_qsv", "h264_amf"],
            ),
            "h264_amf"
        );
        assert_eq!(
            select_windows_video_encoder(
                VideoCodec::H264,
                EncoderMode::Gpu,
                &["libx264", "h264_qsv"],
            ),
            "h264_qsv"
        );
        assert_eq!(
            select_windows_video_encoder(VideoCodec::H264, EncoderMode::Gpu, &["libx264"]),
            "libx264"
        );
        assert_eq!(
            select_windows_video_encoder(VideoCodec::H264, EncoderMode::Cpu, &["h264_nvenc"]),
            "libx264"
        );
    }

    #[test]
    fn select_windows_video_encoder_honours_hevc_and_av1() {
        assert_eq!(
            select_windows_video_encoder(
                VideoCodec::Hevc,
                EncoderMode::Gpu,
                &["hevc_amf", "hevc_qsv"],
            ),
            "hevc_amf"
        );
        assert_eq!(
            select_windows_video_encoder(VideoCodec::Hevc, EncoderMode::Gpu, &[]),
            "libx265"
        );
        assert_eq!(
            select_windows_video_encoder(VideoCodec::Av1, EncoderMode::Gpu, &["av1_nvenc"]),
            "av1_nvenc"
        );
        assert_eq!(
            select_windows_video_encoder(VideoCodec::Av1, EncoderMode::Cpu, &["libsvtav1"]),
            "libsvtav1"
        );
    }

    #[test]
    fn build_windows_ffmpeg_args_single_file_with_audio_pipe() {
        let params = WindowsEncoderParams {
            width: 1920,
            height: 1080,
            fps: 60,
            video_codec: "h264_nvenc".to_string(),
            audio_codec: AudioCodec::Aac,
            quality: QualityPreset::High,
            container: ContainerFormat::Mp4,
            audio_pipe_path: Some(r"\\.\pipe\clipforge-audio-test".to_string()),
            audio_sample_rate: 48_000,
            audio_channels: 2,
            output: WindowsOutputMode::SingleFile(PathBuf::from("C:\\Clips\\rec.mp4")),
        };
        let args: Vec<String> = build_windows_ffmpeg_args(&params)
            .into_iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();

        assert!(args.windows(2).any(|w| w == ["-pixel_format", "bgra"]));
        assert!(args.windows(2).any(|w| w == ["-video_size", "1920x1080"]));
        assert!(args.windows(2).any(|w| w == ["-framerate", "60"]));
        assert!(args.windows(2).any(|w| w == ["-i", "pipe:0"]));
        assert!(args.windows(2).any(|w| w == ["-f", "f32le"]));
        assert!(
            args.windows(2)
                .any(|w| w == ["-i", r"\\.\pipe\clipforge-audio-test"])
        );
        assert!(args.windows(2).any(|w| w == ["-c:v", "h264_nvenc"]));
        assert!(args.windows(2).any(|w| w == ["-c:a", "aac"]));
        assert!(args.windows(2).any(|w| w == ["-movflags", "+faststart"]));
        assert_eq!(args.last().map(String::as_str), Some("C:\\Clips\\rec.mp4"));
    }

    #[test]
    fn build_windows_ffmpeg_args_rolling_segment_without_audio() {
        let params = WindowsEncoderParams {
            width: 2560,
            height: 1440,
            fps: 120,
            video_codec: "libx264".to_string(),
            audio_codec: AudioCodec::Opus,
            quality: QualityPreset::VeryHigh,
            container: ContainerFormat::Mp4,
            audio_pipe_path: None,
            audio_sample_rate: 48_000,
            audio_channels: 2,
            output: WindowsOutputMode::RollingSegment {
                segment_time_secs: 2,
                segment_wrap: 32,
                pattern: PathBuf::from("C:\\Temp\\seg_%03d.mp4"),
            },
        };
        let args: Vec<String> = build_windows_ffmpeg_args(&params)
            .into_iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();

        assert!(!args.iter().any(|a| a == "f32le"));
        assert!(args.windows(2).any(|w| w == ["-f", "segment"]));
        assert!(args.windows(2).any(|w| w == ["-segment_time", "2"]));
        assert!(args.windows(2).any(|w| w == ["-segment_wrap", "32"]));
        assert!(args.windows(2).any(|w| w == ["-reset_timestamps", "1"]));
        assert!(!args.iter().any(|a| a == "+faststart"));
    }

    #[test]
    fn mix_audio_chunks_clamps_and_handles_unequal_lengths() {
        let lb = [0.6_f32, -0.8, 0.25];
        let mic = [0.7_f32, -0.5];
        let mixed = mix_audio_chunks(&lb, &mic);
        assert_eq!(mixed.len(), 3);
        assert!((mixed[0] - 1.0).abs() < 1e-6);
        assert!((mixed[1] - (-1.0)).abs() < 1e-6);
        assert!((mixed[2] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn normalize_channels_and_resample_work() {
        let mono = [0.25_f32, -0.5];
        let stereo = normalize_channels_interleaved(&mono, 1, 2);
        assert_eq!(stereo, vec![0.25, 0.25, -0.5, -0.5]);

        let resampled = resample_linear_interleaved(&stereo, 24_000, 48_000, 2);
        assert_eq!(resampled.len(), 8);
    }
}
