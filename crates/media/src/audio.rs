//! Native audio output (`cpal` via ALSA/PipeWire) and sample-accurate A/V master clock.

use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
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
    cpal_thread: Option<JoinHandle<()>>,
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

        let (init_tx, init_rx) = oneshot::channel::<std::result::Result<(u32, u16), String>>();

        let cpal_stop = Arc::clone(&stop_flag);
        let cpal_played = Arc::clone(&samples_played);
        let cpal_ring = Arc::clone(&ring);
        let cpal_controls = controls.clone();

        let cpal_thread = std::thread::Builder::new()
            .name("clipforge-audio".into())
            .spawn(move || {
                run_cpal_thread(init_tx, cpal_stop, cpal_played, cpal_ring, cpal_controls);
            })
            .map_err(|e| format!("failed to spawn audio thread: {e}"))?;

        let (sample_rate, channels) = match init_rx.await {
            Ok(Ok(cfg)) => cfg,
            Ok(Err(err)) => {
                stop_flag.store(true, Ordering::Relaxed);
                let _ = cpal_thread.join();
                return Err(err);
            }
            Err(_) => {
                stop_flag.store(true, Ordering::Relaxed);
                let _ = cpal_thread.join();
                return Err("audio initialization thread exited unexpectedly".into());
            }
        };

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

        let mut child = Command::new(ffmpeg_bin)
            .args(&ffmpeg_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("failed to spawn ffmpeg audio stream: {e}"))?;

        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill().await;
            stop_flag.store(true, Ordering::Relaxed);
            let _ = cpal_thread.join();
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
            cpal_thread: Some(cpal_thread),
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
        if let Some(thread) = self.cpal_thread.take() {
            let _ = tokio::task::spawn_blocking(move || {
                let _ = thread.join();
            })
            .await;
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
    }
}

fn run_cpal_thread(
    init_tx: oneshot::Sender<std::result::Result<(u32, u16), String>>,
    stop_flag: Arc<AtomicBool>,
    samples_played: Arc<AtomicU64>,
    ring: Arc<Mutex<VecDeque<f32>>>,
    controls: AudioControlState,
) {
    let host = cpal::default_host();
    let Some(device) = host.default_output_device() else {
        let _ = init_tx.send(Err("no default audio output device available".into()));
        return;
    };

    let supported_cfg = match device.default_output_config() {
        Ok(cfg) => cfg,
        Err(e) => {
            let _ = init_tx.send(Err(format!("failed to query default audio config: {e}")));
            return;
        }
    };

    let sample_format = supported_cfg.sample_format();
    let config: StreamConfig = supported_cfg.into();
    let channels = config.channels.max(1);
    let sample_rate = config.sample_rate.0.max(1);

    let err_fn = |err| {
        warn!(error = %err, "cpal output stream error");
    };

    let stream_res = match sample_format {
        SampleFormat::F32 => {
            let ring_cb = Arc::clone(&ring);
            let played_cb = Arc::clone(&samples_played);
            let ctrl_cb = controls.clone();
            device.build_output_stream(
                &config,
                move |data: &mut [f32], _| {
                    fill_audio_buffer(data, channels, &ring_cb, &played_cb, &ctrl_cb, |s| s);
                },
                err_fn,
                None,
            )
        }
        SampleFormat::I16 => {
            let ring_cb = Arc::clone(&ring);
            let played_cb = Arc::clone(&samples_played);
            let ctrl_cb = controls.clone();
            device.build_output_stream(
                &config,
                move |data: &mut [i16], _| {
                    fill_audio_buffer(data, channels, &ring_cb, &played_cb, &ctrl_cb, |s| {
                        (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
                    });
                },
                err_fn,
                None,
            )
        }
        SampleFormat::U16 => {
            let ring_cb = Arc::clone(&ring);
            let played_cb = Arc::clone(&samples_played);
            let ctrl_cb = controls.clone();
            device.build_output_stream(
                &config,
                move |data: &mut [u16], _| {
                    fill_audio_buffer(data, channels, &ring_cb, &played_cb, &ctrl_cb, |s| {
                        ((s.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16
                    });
                },
                err_fn,
                None,
            )
        }
        other => {
            let _ = init_tx.send(Err(format!("unsupported audio sample format: {other:?}")));
            return;
        }
    };

    let stream = match stream_res {
        Ok(s) => s,
        Err(e) => {
            let _ = init_tx.send(Err(format!("failed to build cpal output stream: {e}")));
            return;
        }
    };

    if let Err(e) = stream.play() {
        let _ = init_tx.send(Err(format!("failed to start cpal audio stream: {e}")));
        return;
    }

    let _ = init_tx.send(Ok((sample_rate, channels)));

    while !stop_flag.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(10));
    }

    drop(stream);
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
}
