//! Native audio output (`cpal` via ALSA/PipeWire) and sample-accurate A/V master clock.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::oneshot;
use tokio::task::JoinHandle as TokioJoinHandle;
use tokio::time::Instant;
use tracing::{debug, warn};

use crate::processor::format_timestamp_ms;

const MAX_BUFFER_SECONDS: usize = 1;
const AUDIO_READ_CHUNK_BYTES: usize = 8192;

/// Shared atomic audio controls preserved across seeks and clip opens.
#[derive(Clone, Debug)]
pub struct AudioControlState {
    volume_bits: Arc<AtomicU32>,
    muted: Arc<AtomicBool>,
}

impl Default for AudioControlState {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioControlState {
    pub fn new() -> Self {
        Self {
            volume_bits: Arc::new(AtomicU32::new(1.0_f32.to_bits())),
            muted: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_volume(&self, volume: f32) {
        let clamped = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.volume_bits.store(clamped.to_bits(), Ordering::Relaxed);
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    #[inline]
    fn effective_gain(&self) -> f32 {
        if self.muted.load(Ordering::Relaxed) {
            0.0
        } else {
            f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
        }
    }
}

/// Master clock used to synchronize video frame presentation.
pub(crate) enum PlaybackClock {
    WallClock {
        epoch: Instant,
    },
    Audio {
        sample_rate: u32,
        samples_played: Arc<AtomicU64>,
        audio_eof: Arc<AtomicBool>,
        start_epoch: Instant,
        eof_anchor: Option<(u64, Instant)>,
    },
}

impl PlaybackClock {
    pub(crate) fn wall_clock() -> Self {
        Self::WallClock {
            epoch: Instant::now(),
        }
    }

    /// Returns elapsed playback time in milliseconds since stream start.
    pub(crate) fn elapsed_ms(&mut self) -> f64 {
        match self {
            Self::WallClock { epoch } => epoch.elapsed().as_secs_f64() * 1000.0,
            Self::Audio {
                sample_rate,
                samples_played,
                audio_eof,
                start_epoch,
                eof_anchor,
            } => {
                let played = samples_played.load(Ordering::Relaxed);
                if played == 0 {
                    // Before the first audio buffer is consumed by the hardware,
                    // allow a small ramp so video doesn't stall if device startup takes ~20ms.
                    return (start_epoch.elapsed().as_secs_f64() * 1000.0).min(40.0);
                }

                let audio_ms = (played as f64) * 1000.0 / (*sample_rate as f64);
                if audio_eof.load(Ordering::Relaxed) {
                    let (last_played, anchor_time) = match eof_anchor {
                        Some(pair) if pair.0 == played => *pair,
                        _ => {
                            let pair = (played, Instant::now());
                            *eof_anchor = Some(pair);
                            pair
                        }
                    };
                    let base_ms = (last_played as f64) * 1000.0 / (*sample_rate as f64);
                    base_ms + anchor_time.elapsed().as_secs_f64() * 1000.0
                } else {
                    audio_ms
                }
            }
        }
    }
}

/// Selects how `ffmpeg` decodes audio streams for playback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AudioTrackMode {
    /// Mix all available audio tracks (or play `0:a:0` directly if only 1 track exists).
    AutoMix { track_count: usize },
    /// Decode a single explicit audio track index (`0:a:{index}`).
    Single(usize),
}

/// Builds `ffmpeg` CLI arguments to decode the requested audio track(s) into `f32le` PCM.
pub(crate) fn build_audio_decode_args(
    canonical_in: &Path,
    start_ms: u64,
    trim_end_ms: u64,
    track_mode: AudioTrackMode,
    sample_rate: u32,
    channels: u16,
) -> Vec<String> {
    let start_ts = format_timestamp_ms(start_ms);
    let dur_ts = format_timestamp_ms(trim_end_ms.saturating_sub(start_ms));

    let mut args = vec![
        "-v".to_string(),
        "error".to_string(),
        "-ss".to_string(),
        start_ts,
        "-i".to_string(),
        canonical_in.to_string_lossy().into_owned(),
        "-t".to_string(),
        dur_ts,
        "-vn".to_string(),
    ];

    match track_mode {
        AudioTrackMode::AutoMix { track_count } if track_count > 1 => {
            let mut inputs = String::new();
            for idx in 0..track_count {
                inputs.push_str(&format!("[0:a:{idx}]"));
            }
            let filter = format!(
                "{inputs}amix=inputs={track_count}:duration=longest:dropout_transition=0:normalize=0[aout]"
            );
            args.push("-filter_complex".to_string());
            args.push(filter);
            args.push("-map".to_string());
            args.push("[aout]".to_string());
        }
        AudioTrackMode::AutoMix { .. } => {
            args.push("-map".to_string());
            args.push("0:a:0?".to_string());
        }
        AudioTrackMode::Single(idx) => {
            args.push("-map".to_string());
            args.push(format!("0:a:{idx}?"));
        }
    }

    args.extend([
        "-f".to_string(),
        "f32le".to_string(),
        "-ac".to_string(),
        channels.to_string(),
        "-ar".to_string(),
        sample_rate.to_string(),
        "pipe:1".to_string(),
    ]);
    args
}

/// Active audio decode + `cpal` output session.
pub(crate) struct AudioSession {
    stop_flag: Arc<AtomicBool>,
    feeder_task: Option<TokioJoinHandle<()>>,
    child: Option<Child>,
    stream_guard: Option<CpalStreamGuard>,
}

impl AudioSession {
    /// Attempts to start native `cpal` audio playback + `ffmpeg` PCM decoding.
    ///
    /// On device/initialization failure, returns `Err(reason)` so the caller can
    /// report a non-fatal warning and fall back to the monotonic wall clock.
    pub(crate) async fn start(
        ffmpeg_bin: &Path,
        canonical_in: &Path,
        start_ms: u64,
        trim_end_ms: u64,
        track_mode: AudioTrackMode,
        controls: AudioControlState,
    ) -> std::result::Result<(Self, PlaybackClock), String> {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let samples_played = Arc::new(AtomicU64::new(0));
        let audio_eof = Arc::new(AtomicBool::new(false));
        let ring: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));

        let (stream_guard, sample_rate, channels, _) = start_cpal_output_stream_async(
            None,
            CpalStreamSource::RingBuffer {
                ring: Arc::clone(&ring),
                samples_played: Arc::clone(&samples_played),
                controls: controls.clone(),
            },
        )
        .await?;

        let ffmpeg_args = build_audio_decode_args(
            canonical_in,
            start_ms,
            trim_end_ms,
            track_mode,
            sample_rate,
            channels,
        );

        debug!(
            video = %canonical_in.display(),
            start_ms,
            trim_end_ms,
            ?track_mode,
            sample_rate,
            channels,
            "spawning ffmpeg audio decode stream"
        );

        let mut child = match Command::new(ffmpeg_bin)
            .args(&ffmpeg_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                stream_guard.stop().await;
                return Err(format!("failed to spawn ffmpeg audio stream: {e}"));
            }
        };

        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill().await;
            stream_guard.stop().await;
            return Err("failed to capture ffmpeg audio stdout".into());
        };

        let max_ring_samples = (sample_rate as usize)
            .saturating_mul(channels as usize)
            .saturating_mul(MAX_BUFFER_SECONDS)
            .max(4096);

        let feeder_stop = Arc::clone(&stop_flag);
        let feeder_ring = Arc::clone(&ring);
        let feeder_eof = Arc::clone(&audio_eof);

        let feeder_task = tokio::spawn(async move {
            let mut buf = vec![0u8; AUDIO_READ_CHUNK_BYTES];
            let mut leftover = [0u8; 4];
            let mut leftover_len = 0usize;

            loop {
                if feeder_stop.load(Ordering::Relaxed) {
                    break;
                }

                // Backpressure: keep at most MAX_BUFFER_SECONDS of PCM in memory.
                let current_len = feeder_ring.lock().map(|q| q.len()).unwrap_or(0);
                if current_len >= max_ring_samples {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    continue;
                }

                match stdout.read(&mut buf).await {
                    Ok(0) => {
                        feeder_eof.store(true, Ordering::Relaxed);
                        break;
                    }
                    Ok(n) => {
                        let mut offset = 0usize;
                        let mut samples = Vec::with_capacity((n + leftover_len) / 4);

                        if leftover_len > 0 {
                            let need = 4 - leftover_len;
                            if n >= need {
                                leftover[leftover_len..4].copy_from_slice(&buf[..need]);
                                samples.push(f32::from_le_bytes(leftover));
                                leftover_len = 0;
                                offset = need;
                            } else {
                                leftover[leftover_len..leftover_len + n].copy_from_slice(&buf[..n]);
                                leftover_len += n;
                                continue;
                            }
                        }

                        while offset + 4 <= n {
                            let bytes = [
                                buf[offset],
                                buf[offset + 1],
                                buf[offset + 2],
                                buf[offset + 3],
                            ];
                            samples.push(f32::from_le_bytes(bytes));
                            offset += 4;
                        }

                        if offset < n {
                            leftover_len = n - offset;
                            leftover[..leftover_len].copy_from_slice(&buf[offset..n]);
                        }

                        if !samples.is_empty()
                            && let Ok(mut q) = feeder_ring.lock()
                        {
                            q.extend(samples);
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "ffmpeg audio pipe read error");
                        feeder_eof.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
        });

        let clock = PlaybackClock::Audio {
            sample_rate,
            samples_played,
            audio_eof,
            start_epoch: Instant::now(),
            eof_anchor: None,
        };

        let session = Self {
            stop_flag,
            feeder_task: Some(feeder_task),
            child: Some(child),
            stream_guard: Some(stream_guard),
        };

        Ok((session, clock))
    }

    pub(crate) async fn stop(mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.feeder_task.take() {
            handle.abort();
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        if let Some(guard) = self.stream_guard.take() {
            guard.stop().await;
        }
    }
}

impl Drop for AudioSession {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.feeder_task.take() {
            handle.abort();
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
        }
        // Dropping `self.stream_guard` dispatches `StopStream` to the persistent
        // `"clipforge-cpal"` thread so the `cpal::Stream` is never dropped on a
        // tokio worker thread.
    }
}

pub(crate) enum CpalStreamSource {
    RingBuffer {
        ring: Arc<Mutex<VecDeque<f32>>>,
        samples_played: Arc<AtomicU64>,
        controls: AudioControlState,
    },
    Buffer {
        samples: Vec<f32>,
        src_rate: u32,
        src_channels: u16,
        cursor: Arc<AtomicUsize>,
        finished: Arc<AtomicBool>,
        error_flag: Arc<AtomicBool>,
    },
}

pub(crate) struct CpalStreamGuard {
    id: u64,
    stopped: bool,
}

impl CpalStreamGuard {
    pub(crate) async fn stop(mut self) {
        self.stopped = true;
        if let Ok(worker) = cpal_worker() {
            let (ack_tx, ack_rx) = oneshot::channel();
            if worker
                .cmd_tx
                .send(CpalCommand::StopStream {
                    id: self.id,
                    ack: Some(ack_tx),
                })
                .is_ok()
            {
                let _ = ack_rx.await;
            }
        }
    }

    pub(crate) fn stop_blocking(mut self) {
        self.stopped = true;
        if let Ok(worker) = cpal_worker() {
            let (ack_tx, ack_rx) = oneshot::channel();
            if worker
                .cmd_tx
                .send(CpalCommand::StopStream {
                    id: self.id,
                    ack: Some(ack_tx),
                })
                .is_ok()
            {
                let _ = ack_rx.blocking_recv();
            }
        }
    }
}

impl Drop for CpalStreamGuard {
    fn drop(&mut self) {
        if !self.stopped
            && let Ok(worker) = cpal_worker()
        {
            let _ = worker.cmd_tx.send(CpalCommand::StopStream {
                id: self.id,
                ack: None,
            });
        }
    }
}

enum CpalCommand {
    StartStream {
        id: u64,
        device_name: Option<String>,
        source: CpalStreamSource,
        reply: oneshot::Sender<std::result::Result<(u32, u16, usize), String>>,
    },
    StopStream {
        id: u64,
        ack: Option<oneshot::Sender<()>>,
    },
}

struct CpalWorker {
    cmd_tx: std::sync::mpsc::Sender<CpalCommand>,
}

/// Single persistent OS thread for all `cpal` host/device/stream operations.
///
/// On Windows, `cpal::wasapi` initializes COM as STA on the first thread that touches
/// `IMMDeviceEnumerator` (`static ENUMERATOR: OnceLock<Enumerator>`) and tears down COM
/// via `CoUninitialize()` when that thread exits. Keeping one persistent thread alive for
/// the process lifetime prevents use-after-`CoUninitialize` access violations (`0xC0000005`).
static CPAL_WORKER: OnceLock<std::result::Result<CpalWorker, String>> = OnceLock::new();
static NEXT_STREAM_ID: AtomicU64 = AtomicU64::new(1);

fn cpal_worker() -> std::result::Result<&'static CpalWorker, String> {
    let res = CPAL_WORKER.get_or_init(|| {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<CpalCommand>();
        std::thread::Builder::new()
            .name("clipforge-cpal".into())
            .spawn(move || {
                run_cpal_worker_loop(cmd_rx);
            })
            .map_err(|e| format!("failed to spawn persistent cpal audio thread: {e}"))?;
        Ok(CpalWorker { cmd_tx })
    });
    res.as_ref().map_err(Clone::clone)
}

fn run_cpal_worker_loop(cmd_rx: std::sync::mpsc::Receiver<CpalCommand>) {
    let mut streams: HashMap<u64, cpal::Stream> = HashMap::new();
    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            CpalCommand::StartStream {
                id,
                device_name,
                source,
                reply,
            } => {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    open_and_play_cpal_stream(device_name.as_deref(), source)
                }));
                match res {
                    Ok(Ok((maybe_stream, sample_rate, channels, total_samples))) => {
                        if let Some(stream) = maybe_stream {
                            streams.insert(id, stream);
                        }
                        let _ = reply.send(Ok((sample_rate, channels, total_samples)));
                    }
                    Ok(Err(err)) => {
                        let _ = reply.send(Err(err));
                    }
                    Err(_) => {
                        let _ = reply.send(Err(
                            "cpal audio backend panicked during stream initialization".into(),
                        ));
                    }
                }
            }
            CpalCommand::StopStream { id, ack } => {
                if let Some(stream) = streams.remove(&id) {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        drop(stream);
                    }));
                }
                if let Some(ack_tx) = ack {
                    let _ = ack_tx.send(());
                }
            }
        }
    }
}

pub(crate) async fn start_cpal_output_stream_async(
    device_name: Option<String>,
    source: CpalStreamSource,
) -> std::result::Result<(CpalStreamGuard, u32, u16, usize), String> {
    let worker = cpal_worker()?;
    let id = NEXT_STREAM_ID.fetch_add(1, Ordering::Relaxed);
    let (reply_tx, reply_rx) = oneshot::channel();
    worker
        .cmd_tx
        .send(CpalCommand::StartStream {
            id,
            device_name,
            source,
            reply: reply_tx,
        })
        .map_err(|_| "persistent cpal audio thread is unavailable".to_string())?;

    let (sample_rate, channels, total_samples) = reply_rx
        .await
        .map_err(|_| "persistent cpal audio thread exited unexpectedly".to_string())??;

    Ok((
        CpalStreamGuard { id, stopped: false },
        sample_rate,
        channels,
        total_samples,
    ))
}

pub(crate) fn start_cpal_output_stream_blocking(
    device_name: Option<String>,
    source: CpalStreamSource,
) -> std::result::Result<(CpalStreamGuard, u32, u16, usize), String> {
    let worker = cpal_worker()?;
    let id = NEXT_STREAM_ID.fetch_add(1, Ordering::Relaxed);
    let (reply_tx, reply_rx) = oneshot::channel();
    worker
        .cmd_tx
        .send(CpalCommand::StartStream {
            id,
            device_name,
            source,
            reply: reply_tx,
        })
        .map_err(|_| "persistent cpal audio thread is unavailable".to_string())?;

    let (sample_rate, channels, total_samples) = reply_rx
        .blocking_recv()
        .map_err(|_| "persistent cpal audio thread exited unexpectedly".to_string())??;

    Ok((
        CpalStreamGuard { id, stopped: false },
        sample_rate,
        channels,
        total_samples,
    ))
}

trait SampleConvert: cpal::SizedSample + Default {
    fn from_f32_sample(s: f32) -> Self;
}

impl SampleConvert for f32 {
    #[inline]
    fn from_f32_sample(s: f32) -> Self {
        s
    }
}

impl SampleConvert for i16 {
    #[inline]
    fn from_f32_sample(s: f32) -> Self {
        (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
    }
}

impl SampleConvert for u16 {
    #[inline]
    fn from_f32_sample(s: f32) -> Self {
        (((s.clamp(-1.0, 1.0) + 1.0) * 0.5) * u16::MAX as f32) as u16
    }
}

fn build_typed_output_stream<T: SampleConvert>(
    device: &cpal::Device,
    config: &StreamConfig,
    channels: u16,
    sample_rate: u32,
    source: CpalStreamSource,
) -> std::result::Result<(Option<cpal::Stream>, usize), String> {
    match source {
        CpalStreamSource::RingBuffer {
            ring,
            samples_played,
            controls,
        } => {
            let err_fn = |err| {
                warn!(error = %err, "cpal output stream error");
            };
            let stream = device
                .build_output_stream(
                    config,
                    move |data: &mut [T], _| {
                        fill_audio_buffer(
                            data,
                            channels,
                            &ring,
                            &samples_played,
                            &controls,
                            T::from_f32_sample,
                        );
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("failed to build cpal output stream: {e}"))?;
            Ok((Some(stream), 0))
        }
        CpalStreamSource::Buffer {
            samples,
            src_rate,
            src_channels,
            cursor,
            finished,
            error_flag,
        } => {
            let resampled = crate::mic_test::resample_for_output(
                &samples,
                src_rate,
                src_channels,
                sample_rate,
                channels,
            );
            if resampled.is_empty() {
                return Ok((None, 0));
            }
            let total_samples = resampled.len();
            let data = Arc::new(resampled);
            let err_fn = move |err| {
                warn!(error = %err, "mic test cpal playback error");
                error_flag.store(true, Ordering::Relaxed);
            };
            let stream = device
                .build_output_stream(
                    config,
                    move |out: &mut [T], _| {
                        crate::mic_test::write_playback_slice(
                            out,
                            &data,
                            &cursor,
                            &finished,
                            T::from_f32_sample,
                        );
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("failed to build output stream: {e}"))?;
            Ok((Some(stream), total_samples))
        }
    }
}

fn open_and_play_cpal_stream(
    output_device_name: Option<&str>,
    source: CpalStreamSource,
) -> std::result::Result<(Option<cpal::Stream>, u32, u16, usize), String> {
    let host = cpal::default_host();
    let device = match output_device_name {
        Some(name) if !name.trim().is_empty() => host
            .output_devices()
            .map_err(|e| format!("failed to enumerate output devices: {e}"))?
            .find(|d| d.name().ok().as_deref() == Some(name))
            .ok_or_else(|| format!("selected output device '{name}' not found"))?,
        _ => host
            .default_output_device()
            .ok_or_else(|| "no default audio output device available".to_string())?,
    };

    let supported_cfg = device
        .default_output_config()
        .map_err(|e| format!("failed to query default audio config: {e}"))?;

    let sample_format = supported_cfg.sample_format();
    let config: StreamConfig = supported_cfg.into();
    let channels = config.channels.max(1);
    let sample_rate = config.sample_rate.0.max(1);

    let (maybe_stream, total_samples) = match sample_format {
        SampleFormat::F32 => {
            build_typed_output_stream::<f32>(&device, &config, channels, sample_rate, source)?
        }
        SampleFormat::I16 => {
            build_typed_output_stream::<i16>(&device, &config, channels, sample_rate, source)?
        }
        SampleFormat::U16 => {
            build_typed_output_stream::<u16>(&device, &config, channels, sample_rate, source)?
        }
        other => {
            return Err(format!("unsupported audio sample format: {other:?}"));
        }
    };

    if let Some(stream) = &maybe_stream {
        stream
            .play()
            .map_err(|e| format!("failed to start cpal audio stream: {e}"))?;
    }

    Ok((maybe_stream, sample_rate, channels, total_samples))
}

fn fill_audio_buffer<T: Copy>(
    out: &mut [T],
    channels: u16,
    ring: &Arc<Mutex<VecDeque<f32>>>,
    samples_played: &Arc<AtomicU64>,
    controls: &AudioControlState,
    convert: impl Fn(f32) -> T,
) {
    let gain = controls.effective_gain();
    let silence = convert(0.0);
    let mut popped_samples = 0usize;

    if let Ok(mut q) = ring.try_lock() {
        for slot in out.iter_mut() {
            if let Some(sample) = q.pop_front() {
                *slot = convert(sample * gain);
                popped_samples += 1;
            } else {
                *slot = silence;
            }
        }
    } else {
        out.fill(silence);
    }

    if popped_samples > 0 {
        let frames = (popped_samples / (channels.max(1) as usize)) as u64;
        if frames > 0 {
            samples_played.fetch_add(frames, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_audio_decode_args_auto_mixes_multiple_tracks() {
        let args = build_audio_decode_args(
            Path::new("/tmp/test.mp4"),
            0,
            5000,
            AudioTrackMode::AutoMix { track_count: 2 },
            48_000,
            2,
        );
        assert!(args.windows(2).any(|w| {
            w[0] == "-filter_complex"
                && w[1] == "[0:a:0][0:a:1]amix=inputs=2:duration=longest:dropout_transition=0:normalize=0[aout]"
        }));
        assert!(args.windows(2).any(|w| w == ["-map", "[aout]"]));
    }

    #[test]
    fn build_audio_decode_args_single_track_uses_direct_map() {
        let auto_one = build_audio_decode_args(
            Path::new("/tmp/test.mp4"),
            0,
            5000,
            AudioTrackMode::AutoMix { track_count: 1 },
            48_000,
            2,
        );
        assert!(auto_one.windows(2).any(|w| w == ["-map", "0:a:0?"]));
        assert!(!auto_one.iter().any(|a| a == "-filter_complex"));

        let explicit = build_audio_decode_args(
            Path::new("/tmp/test.mp4"),
            0,
            5000,
            AudioTrackMode::Single(1),
            48_000,
            2,
        );
        assert!(explicit.windows(2).any(|w| w == ["-map", "0:a:1?"]));
        assert!(!explicit.iter().any(|a| a == "-filter_complex"));
    }

    #[tokio::test]
    async fn cpal_worker_handles_repeated_and_concurrent_open_close_drop() {
        for i in 0..5 {
            let res = start_cpal_output_stream_async(
                None,
                CpalStreamSource::RingBuffer {
                    ring: Arc::new(Mutex::new(VecDeque::new())),
                    samples_played: Arc::new(AtomicU64::new(0)),
                    controls: AudioControlState::new(),
                },
            )
            .await;

            if let Ok((guard, rate, ch, _)) = res {
                assert!(rate > 0);
                assert!(ch > 0);
                if i % 2 == 0 {
                    guard.stop().await;
                } else {
                    drop(guard);
                }
            }
        }

        // Also test blocking entry point from a worker thread (used by mic_test playback).
        let blocking_handle = std::thread::spawn(|| {
            for i in 0..3 {
                let res = start_cpal_output_stream_blocking(
                    None,
                    CpalStreamSource::Buffer {
                        samples: vec![0.0; 256],
                        src_rate: 48_000,
                        src_channels: 1,
                        cursor: Arc::new(AtomicUsize::new(0)),
                        finished: Arc::new(AtomicBool::new(false)),
                        error_flag: Arc::new(AtomicBool::new(false)),
                    },
                );
                if let Ok((guard, rate, ch, _)) = res {
                    assert!(rate > 0);
                    assert!(ch > 0);
                    if i % 2 == 0 {
                        guard.stop_blocking();
                    } else {
                        drop(guard);
                    }
                }
            }
        });
        blocking_handle.join().expect("blocking cpal thread");
    }
}
