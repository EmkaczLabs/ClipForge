//! Standalone Microphone Test backend using PipeWire/PulseAudio (`parec`/`pactl`) and `cpal` playback.
//!
//! Operates independently from `gpu-screen-recorder` so testing or metering a microphone
//! never interrupts or mutates an active replay buffer.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, warn};

use crate::audio::{CpalStreamSource, start_cpal_output_stream_blocking};

/// Noise/silence floor in dBFS used when signal amplitude is zero or near-zero.
pub const DBFS_FLOOR: f32 = -96.0;
/// Default microphone sample recording duration.
pub const DEFAULT_MIC_TEST_DURATION: Duration = Duration::from_secs(3);
const EVENT_CHANNEL_CAPACITY: usize = 32;
const LEVEL_EMIT_INTERVAL_MS: u64 = 50;

/// Current lifecycle phase of the microphone test service.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MicTestPhase {
    Idle,
    Recording,
    Recorded,
    Playing,
    Completed,
    Error,
}

/// Truthful diagnostic classification for microphone capture and playback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MicFeedbackKind {
    Silence,
    MutedOrVeryQuiet,
    GoodSignal,
    Clipping,
    MissingDevice,
    PermissionError,
    Success,
}

/// Diagnostic summary produced after recording/monitoring a microphone sample.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MicDiagnosticSummary {
    pub selected_source_id: String,
    pub resolved_backend_device: Option<String>,
    pub backend_kind: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub rms_dbfs: f32,
    pub peak_dbfs: f32,
    pub signal_detected: bool,
    pub source_muted: Option<bool>,
    pub recorded_duration_ms: u64,
    pub feedback: MicFeedbackKind,
    pub message: String,
}

/// Events emitted by [`MicTestHandle`] to UI/worker consumers.
#[derive(Clone, Debug)]
pub enum MicTestEvent {
    PhaseChanged(MicTestPhase),
    Level {
        rms_dbfs: f32,
        peak_dbfs: f32,
        elapsed_ms: u64,
    },
    Recorded {
        summary: MicDiagnosticSummary,
    },
    PlaybackFinished,
    Error {
        feedback: MicFeedbackKind,
        message: String,
        summary: Option<MicDiagnosticSummary>,
    },
}

#[derive(Debug)]
enum MicTestCommand {
    StartTest {
        source_id: String,
        duration: Option<Duration>,
    },
    StartMonitor {
        source_id: String,
    },
    Stop,
    PlaySample {
        output_device: Option<String>,
    },
    Clear,
}

/// Handle to the background microphone test worker.
///
/// Dropping the handle cancels any active capture or playback and terminates child processes.
#[derive(Clone)]
pub struct MicTestHandle {
    cmd_tx: mpsc::UnboundedSender<MicTestCommand>,
    shutdown_flag: Arc<AtomicBool>,
}

impl Drop for MicTestHandle {
    fn drop(&mut self) {
        // Only signal shutdown when the last clone is dropped? No, MicTestHandle is typically
        // owned by the worker, and if dropped we let cmd_tx close naturally; if all senders drop,
        // the worker exits and sets its own stop flags. We keep shutdown_flag for explicit close.
    }
}

impl MicTestHandle {
    /// Spawns a new microphone test worker using system `pactl` and `parec` binaries.
    pub fn spawn() -> (Self, mpsc::Receiver<MicTestEvent>) {
        Self::spawn_with_binaries("pactl", "parec")
    }

    /// Spawns a new microphone test worker with explicit `pactl` and `parec` binary paths.
    pub fn spawn_with_binaries(
        pactl_bin: impl Into<PathBuf>,
        parec_bin: impl Into<PathBuf>,
    ) -> (Self, mpsc::Receiver<MicTestEvent>) {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let shutdown_flag = Arc::new(AtomicBool::new(false));

        let worker = MicTestWorker::new(
            pactl_bin.into(),
            parec_bin.into(),
            event_tx,
            Arc::clone(&shutdown_flag),
        );
        tokio::spawn(async move {
            worker.run(cmd_rx).await;
        });

        (
            Self {
                cmd_tx,
                shutdown_flag,
            },
            event_rx,
        )
    }

    /// Starts a bounded microphone recording test (defaults to 3s when `duration` is `None`).
    pub fn start_test(&self, source_id: impl Into<String>, duration: Option<Duration>) {
        let _ = self.cmd_tx.send(MicTestCommand::StartTest {
            source_id: source_id.into(),
            duration: Some(duration.unwrap_or(DEFAULT_MIC_TEST_DURATION)),
        });
    }

    /// Starts continuous live dBFS level monitoring for `source_id` until [`Self::stop`] is called.
    pub fn start_monitor(&self, source_id: impl Into<String>) {
        let _ = self.cmd_tx.send(MicTestCommand::StartMonitor {
            source_id: source_id.into(),
        });
    }

    /// Immediately stops any active recording, monitoring, or playback and cleans up streams.
    pub fn stop(&self) {
        let _ = self.cmd_tx.send(MicTestCommand::Stop);
    }

    /// Plays back the most recently recorded microphone sample on the default (or named) output device.
    pub fn play_sample(&self, output_device: Option<String>) {
        let _ = self
            .cmd_tx
            .send(MicTestCommand::PlaySample { output_device });
    }

    /// Stops any active operation and clears any recorded sample buffer.
    pub fn clear(&self) {
        let _ = self.cmd_tx.send(MicTestCommand::Clear);
    }

    /// Signals immediate shutdown of the worker and any active subprocesses/streams.
    pub fn shutdown(&self) {
        self.shutdown_flag.store(true, Ordering::Relaxed);
        let _ = self.cmd_tx.send(MicTestCommand::Clear);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PactlSourceEntry {
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Clone, Debug)]
struct RecordedSampleBuffer {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
    summary: MicDiagnosticSummary,
}

struct ActiveCapture {
    selected_source_id: String,
    resolved_device: String,
    sample_rate: u32,
    channels: u16,
    source_muted: Option<bool>,
    is_recording: bool,
    max_samples: Option<usize>,
    started_at: Instant,
    child: Child,
    stdout: tokio::process::ChildStdout,
    leftover: [u8; 4],
    leftover_len: usize,
    window_samples: Vec<f32>,
    recorded_samples: Vec<f32>,
    last_level_emit: Instant,
}

impl ActiveCapture {
    async fn stop_and_reap(mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

struct ActivePlayback {
    stop_flag: Arc<AtomicBool>,
    done_rx: oneshot::Receiver<std::result::Result<(), String>>,
    thread: Option<JoinHandle<()>>,
}

impl ActivePlayback {
    fn stop_and_join(mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct MicTestWorker {
    pactl_bin: PathBuf,
    parec_bin: PathBuf,
    event_tx: mpsc::Sender<MicTestEvent>,
    shutdown_flag: Arc<AtomicBool>,
    capture: Option<ActiveCapture>,
    playback: Option<ActivePlayback>,
    last_recording: Option<RecordedSampleBuffer>,
}

impl MicTestWorker {
    fn new(
        pactl_bin: PathBuf,
        parec_bin: PathBuf,
        event_tx: mpsc::Sender<MicTestEvent>,
        shutdown_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            pactl_bin,
            parec_bin,
            event_tx,
            shutdown_flag,
            capture: None,
            playback: None,
            last_recording: None,
        }
    }

    async fn emit(&self, event: MicTestEvent) {
        let _ = self.event_tx.send(event).await;
    }

    async fn run(mut self, mut cmd_rx: mpsc::UnboundedReceiver<MicTestCommand>) {
        loop {
            if self.shutdown_flag.load(Ordering::Relaxed) {
                break;
            }

            if let Some(cap) = self.capture.as_mut() {
                let mut buf = [0u8; 4096];
                tokio::select! {
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(c) => self.handle_command(c).await,
                            None => break,
                        }
                    }
                    read_res = cap.stdout.read(&mut buf) => {
                        match read_res {
                            Ok(0) => {
                                self.handle_capture_eof().await;
                            }
                            Ok(n) => {
                                self.process_capture_bytes(&buf[..n]).await;
                            }
                            Err(e) => {
                                warn!(error = %e, "microphone capture read error");
                                self.abort_capture_with_error(
                                    MicFeedbackKind::MissingDevice,
                                    format!("Microphone capture stream interrupted: {e}"),
                                ).await;
                            }
                        }
                    }
                }
            } else if let Some(pb) = self.playback.as_mut() {
                tokio::select! {
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(c) => self.handle_command(c).await,
                            None => break,
                        }
                    }
                    res = &mut pb.done_rx => {
                        let pb = self.playback.take().unwrap();
                        if let Some(t) = pb.thread {
                            let _ = t.join();
                        }
                        match res {
                            Ok(Ok(())) => {
                                self.emit(MicTestEvent::PlaybackFinished).await;
                                self.emit(MicTestEvent::PhaseChanged(MicTestPhase::Completed)).await;
                            }
                            Ok(Err(err)) => {
                                self.emit(MicTestEvent::Error {
                                    feedback: MicFeedbackKind::MissingDevice,
                                    message: err,
                                    summary: self.last_recording.as_ref().map(|r| r.summary.clone()),
                                }).await;
                                self.emit(MicTestEvent::PhaseChanged(MicTestPhase::Error)).await;
                            }
                            Err(_) => {
                                self.emit(MicTestEvent::PlaybackFinished).await;
                                self.emit(MicTestEvent::PhaseChanged(MicTestPhase::Completed)).await;
                            }
                        }
                    }
                }
            } else {
                match cmd_rx.recv().await {
                    Some(c) => self.handle_command(c).await,
                    None => break,
                }
            }
        }

        self.cleanup_active().await;
    }

    async fn cleanup_active(&mut self) {
        if let Some(cap) = self.capture.take() {
            cap.stop_and_reap().await;
        }
        if let Some(pb) = self.playback.take() {
            pb.stop_and_join();
        }
    }

    async fn handle_command(&mut self, cmd: MicTestCommand) {
        match cmd {
            MicTestCommand::StartTest {
                source_id,
                duration,
            } => {
                self.start_capture(source_id, duration).await;
            }
            MicTestCommand::StartMonitor { source_id } => {
                self.start_capture(source_id, None).await;
            }
            MicTestCommand::Stop => {
                if let Some(cap) = self.capture.take() {
                    if cap.is_recording && !cap.recorded_samples.is_empty() {
                        self.finish_recording(cap).await;
                    } else {
                        cap.stop_and_reap().await;
                        self.emit(MicTestEvent::PhaseChanged(MicTestPhase::Idle))
                            .await;
                    }
                } else if let Some(pb) = self.playback.take() {
                    pb.stop_and_join();
                    let next_phase = if self.last_recording.is_some() {
                        MicTestPhase::Recorded
                    } else {
                        MicTestPhase::Idle
                    };
                    self.emit(MicTestEvent::PhaseChanged(next_phase)).await;
                }
            }
            MicTestCommand::PlaySample { output_device } => {
                self.start_playback(output_device).await;
            }
            MicTestCommand::Clear => {
                self.cleanup_active().await;
                self.last_recording = None;
                self.emit(MicTestEvent::PhaseChanged(MicTestPhase::Idle))
                    .await;
            }
        }
    }

    async fn start_capture(&mut self, source_id: String, duration: Option<Duration>) {
        self.cleanup_active().await;

        let resolved = match resolve_pipewire_source(&self.pactl_bin, &source_id).await {
            Ok(r) => r,
            Err((feedback, msg)) => {
                let summary = MicDiagnosticSummary {
                    selected_source_id: source_id,
                    resolved_backend_device: None,
                    backend_kind: "pipewire-pulse (parec)".to_string(),
                    sample_rate: 0,
                    channels: 0,
                    rms_dbfs: DBFS_FLOOR,
                    peak_dbfs: DBFS_FLOOR,
                    signal_detected: false,
                    source_muted: None,
                    recorded_duration_ms: 0,
                    feedback,
                    message: msg.clone(),
                };
                self.emit(MicTestEvent::Error {
                    feedback,
                    message: msg,
                    summary: Some(summary),
                })
                .await;
                self.emit(MicTestEvent::PhaseChanged(MicTestPhase::Error))
                    .await;
                return;
            }
        };

        let mut cmd = Command::new(&self.parec_bin);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000);
        let mut child = match cmd
            .arg(format!("--device={}", resolved.name))
            .arg("--format=float32le")
            .arg(format!("--rate={}", resolved.sample_rate))
            .arg(format!("--channels={}", resolved.channels))
            .arg("--latency-msec=20")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let feedback = classify_spawn_error(&e);
                let msg = format!("Failed to start microphone capture (`parec`): {e}");
                self.emit(MicTestEvent::Error {
                    feedback,
                    message: msg,
                    summary: None,
                })
                .await;
                self.emit(MicTestEvent::PhaseChanged(MicTestPhase::Error))
                    .await;
                return;
            }
        };

        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill().await;
            let _ = child.wait().await;
            self.emit(MicTestEvent::Error {
                feedback: MicFeedbackKind::MissingDevice,
                message: "Failed to capture `parec` stdout".to_string(),
                summary: None,
            })
            .await;
            self.emit(MicTestEvent::PhaseChanged(MicTestPhase::Error))
                .await;
            return;
        };

        let max_samples = duration.map(|d| {
            let total = (resolved.sample_rate as u128)
                .saturating_mul(resolved.channels as u128)
                .saturating_mul(d.as_millis())
                / 1000;
            (total as usize).max(resolved.channels as usize)
        });

        debug!(
            source_id = %source_id,
            resolved = %resolved.name,
            sample_rate = resolved.sample_rate,
            channels = resolved.channels,
            muted = ?resolved.muted,
            "started microphone test capture"
        );

        let now = Instant::now();
        self.capture = Some(ActiveCapture {
            selected_source_id: source_id,
            resolved_device: resolved.name,
            sample_rate: resolved.sample_rate,
            channels: resolved.channels,
            source_muted: resolved.muted,
            is_recording: duration.is_some(),
            max_samples,
            started_at: now,
            child,
            stdout,
            leftover: [0u8; 4],
            leftover_len: 0,
            window_samples: Vec::with_capacity(4096),
            recorded_samples: Vec::new(),
            last_level_emit: now,
        });

        self.emit(MicTestEvent::PhaseChanged(MicTestPhase::Recording))
            .await;
    }

    async fn process_capture_bytes(&mut self, bytes: &[u8]) {
        let Some(cap) = self.capture.as_mut() else {
            return;
        };

        let mut offset = 0usize;
        if cap.leftover_len > 0 {
            let needed = 4 - cap.leftover_len;
            if bytes.len() < needed {
                cap.leftover[cap.leftover_len..cap.leftover_len + bytes.len()]
                    .copy_from_slice(bytes);
                cap.leftover_len += bytes.len();
                return;
            }
            cap.leftover[cap.leftover_len..4].copy_from_slice(&bytes[..needed]);
            let sample = f32::from_le_bytes(cap.leftover);
            cap.window_samples.push(sample);
            if cap.is_recording {
                cap.recorded_samples.push(sample);
            }
            cap.leftover_len = 0;
            offset = needed;
        }

        let remaining = &bytes[offset..];
        let (chunks, tail) = remaining.as_chunks::<4>();
        for &chunk in chunks {
            let sample = f32::from_le_bytes(chunk);
            cap.window_samples.push(sample);
            if cap.is_recording {
                cap.recorded_samples.push(sample);
            }
        }
        if !tail.is_empty() {
            cap.leftover[..tail.len()].copy_from_slice(tail);
            cap.leftover_len = tail.len();
        }

        let should_emit_level =
            cap.last_level_emit.elapsed() >= Duration::from_millis(LEVEL_EMIT_INTERVAL_MS);
        let reached_limit = cap
            .max_samples
            .is_some_and(|max| cap.recorded_samples.len() >= max);

        if should_emit_level && !cap.window_samples.is_empty() {
            let (rms_dbfs, peak_dbfs) = compute_dbfs(&cap.window_samples);
            let elapsed_ms = cap.started_at.elapsed().as_millis() as u64;
            cap.window_samples.clear();
            cap.last_level_emit = Instant::now();
            self.emit(MicTestEvent::Level {
                rms_dbfs,
                peak_dbfs,
                elapsed_ms,
            })
            .await;
        }

        if reached_limit {
            let cap = self.capture.take().unwrap();
            self.finish_recording(cap).await;
        }
    }

    async fn handle_capture_eof(&mut self) {
        let Some(mut cap) = self.capture.take() else {
            return;
        };

        let mut stderr_msg = String::new();
        if let Some(mut stderr) = cap.child.stderr.take() {
            let _ = stderr.read_to_string(&mut stderr_msg).await;
        }
        let status = cap.child.wait().await.ok();

        if cap.is_recording && !cap.recorded_samples.is_empty() {
            self.finish_recording_from_samples(
                cap.selected_source_id,
                cap.resolved_device,
                cap.sample_rate,
                cap.channels,
                cap.source_muted,
                cap.recorded_samples,
            )
            .await;
            return;
        }

        let feedback = classify_stderr_error(&stderr_msg);
        let msg = if stderr_msg.trim().is_empty() {
            format!(
                "Microphone capture ended unexpectedly (status: {:?})",
                status.and_then(|s| s.code())
            )
        } else {
            stderr_msg.trim().to_string()
        };
        self.emit(MicTestEvent::Error {
            feedback,
            message: msg,
            summary: None,
        })
        .await;
        self.emit(MicTestEvent::PhaseChanged(MicTestPhase::Error))
            .await;
    }

    async fn abort_capture_with_error(&mut self, feedback: MicFeedbackKind, message: String) {
        if let Some(cap) = self.capture.take() {
            cap.stop_and_reap().await;
        }
        self.emit(MicTestEvent::Error {
            feedback,
            message,
            summary: None,
        })
        .await;
        self.emit(MicTestEvent::PhaseChanged(MicTestPhase::Error))
            .await;
    }

    async fn finish_recording(&mut self, cap: ActiveCapture) {
        let selected_source_id = cap.selected_source_id.clone();
        let resolved_device = cap.resolved_device.clone();
        let sample_rate = cap.sample_rate;
        let channels = cap.channels;
        let source_muted = cap.source_muted;
        let samples = cap.recorded_samples.clone();
        cap.stop_and_reap().await;

        self.finish_recording_from_samples(
            selected_source_id,
            resolved_device,
            sample_rate,
            channels,
            source_muted,
            samples,
        )
        .await;
    }

    async fn finish_recording_from_samples(
        &mut self,
        selected_source_id: String,
        resolved_device: String,
        sample_rate: u32,
        channels: u16,
        source_muted: Option<bool>,
        samples: Vec<f32>,
    ) {
        let (rms_dbfs, peak_dbfs) = compute_dbfs(&samples);
        let signal_detected = peak_dbfs > -70.0;
        let (feedback, message) = classify_mic_signal(rms_dbfs, peak_dbfs, source_muted);
        let denom = (sample_rate as u64).saturating_mul(channels as u64).max(1);
        let recorded_duration_ms = (samples.len() as u64).saturating_mul(1000) / denom;

        let summary = MicDiagnosticSummary {
            selected_source_id,
            resolved_backend_device: Some(resolved_device),
            backend_kind: "pipewire-pulse (parec)".to_string(),
            sample_rate,
            channels,
            rms_dbfs,
            peak_dbfs,
            signal_detected,
            source_muted,
            recorded_duration_ms,
            feedback,
            message,
        };

        self.last_recording = Some(RecordedSampleBuffer {
            samples,
            sample_rate,
            channels,
            summary: summary.clone(),
        });

        self.emit(MicTestEvent::Recorded { summary }).await;
        self.emit(MicTestEvent::PhaseChanged(MicTestPhase::Recorded))
            .await;
    }

    async fn start_playback(&mut self, output_device: Option<String>) {
        self.cleanup_active().await;

        let Some(rec) = self.last_recording.clone() else {
            self.emit(MicTestEvent::Error {
                feedback: MicFeedbackKind::Silence,
                message: "No recorded microphone sample available to play back".to_string(),
                summary: None,
            })
            .await;
            return;
        };

        let stop_flag = Arc::new(AtomicBool::new(false));
        let (done_tx, done_rx) = oneshot::channel();
        let thread_stop = Arc::clone(&stop_flag);

        let thread = std::thread::Builder::new()
            .name("clipforge-mic-playback".into())
            .spawn(move || {
                let res = play_samples_cpal(
                    &rec.samples,
                    rec.sample_rate,
                    rec.channels,
                    output_device.as_deref(),
                    thread_stop,
                );
                let _ = done_tx.send(res);
            })
            .ok();

        let Some(thread) = thread else {
            self.emit(MicTestEvent::Error {
                feedback: MicFeedbackKind::MissingDevice,
                message: "Failed to spawn microphone playback thread".to_string(),
                summary: Some(rec.summary),
            })
            .await;
            return;
        };

        self.playback = Some(ActivePlayback {
            stop_flag,
            done_rx,
            thread: Some(thread),
        });
        self.emit(MicTestEvent::PhaseChanged(MicTestPhase::Playing))
            .await;
    }
}

/// Computes `(rms_dbfs, peak_dbfs)` for a slice of normalized `f32` PCM samples.
///
/// Values are clamped to `[DBFS_FLOOR, 0.0]` so silence produces `-96.0 dBFS` rather than `-inf`.
pub fn compute_dbfs(samples: &[f32]) -> (f32, f32) {
    if samples.is_empty() {
        return (DBFS_FLOOR, DBFS_FLOOR);
    }

    let mut sum_sq = 0.0_f64;
    let mut peak = 0.0_f32;
    let mut count = 0usize;

    for &s in samples {
        if s.is_finite() {
            let abs = s.abs();
            if abs > peak {
                peak = abs;
            }
            sum_sq += (s as f64) * (s as f64);
            count += 1;
        }
    }

    if count == 0 {
        return (DBFS_FLOOR, DBFS_FLOOR);
    }

    let rms = (sum_sq / count as f64).sqrt() as f32;
    let rms_dbfs = if rms <= 1e-5 {
        DBFS_FLOOR
    } else {
        (20.0 * rms.log10()).clamp(DBFS_FLOOR, 0.0)
    };
    let peak_dbfs = if peak <= 1e-5 {
        DBFS_FLOOR
    } else {
        (20.0 * peak.log10()).clamp(DBFS_FLOOR, 0.0)
    };
    (rms_dbfs, peak_dbfs)
}

/// Classifies recorded/monitored microphone levels into truthful user-facing feedback.
pub fn classify_mic_signal(
    rms_dbfs: f32,
    peak_dbfs: f32,
    source_muted: Option<bool>,
) -> (MicFeedbackKind, String) {
    if peak_dbfs >= -1.0 {
        (
            MicFeedbackKind::Clipping,
            format!("Signal clipping detected (peak {peak_dbfs:.1} dBFS). Lower input gain."),
        )
    } else if rms_dbfs <= -80.0 && peak_dbfs <= -70.0 {
        if source_muted == Some(true) {
            (
                MicFeedbackKind::MutedOrVeryQuiet,
                "Selected microphone is muted in PipeWire/PulseAudio.".to_string(),
            )
        } else {
            (
                MicFeedbackKind::Silence,
                "No microphone signal detected (silence). Check mute switch or input.".to_string(),
            )
        }
    } else if rms_dbfs < -45.0 {
        (
            MicFeedbackKind::MutedOrVeryQuiet,
            format!(
                "Microphone signal is very quiet (RMS {rms_dbfs:.1} dBFS). Increase gain or speak closer."
            ),
        )
    } else {
        (
            MicFeedbackKind::GoodSignal,
            format!("Good microphone signal (RMS {rms_dbfs:.1} dBFS, peak {peak_dbfs:.1} dBFS)."),
        )
    }
}

#[derive(Clone, Debug)]
struct ResolvedSource {
    name: String,
    sample_rate: u32,
    channels: u16,
    muted: Option<bool>,
}

async fn resolve_pipewire_source(
    pactl_bin: &Path,
    source_id: &str,
) -> std::result::Result<ResolvedSource, (MicFeedbackKind, String)> {
    let trimmed = source_id.trim();
    if trimmed.is_empty() {
        return Err((
            MicFeedbackKind::MissingDevice,
            "No microphone source selected.".to_string(),
        ));
    }

    let normalized = trimmed.strip_prefix("device:").unwrap_or(trimmed);

    let mut list_cmd = Command::new(pactl_bin);
    #[cfg(windows)]
    list_cmd.creation_flags(0x0800_0000);
    let list_out = list_cmd
        .args(["list", "sources", "short"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| {
            (
                classify_spawn_error(&e),
                format!("Failed to query PipeWire/PulseAudio sources (`pactl`): {e}"),
            )
        })?;

    if !list_out.status.success() {
        let stderr = String::from_utf8_lossy(&list_out.stderr);
        return Err((
            classify_stderr_error(&stderr),
            format!("Audio server query failed: {}", stderr.trim()),
        ));
    }

    let stdout = String::from_utf8_lossy(&list_out.stdout);
    let entries = parse_pactl_sources_short(&stdout);

    let target_name = if normalized == "default_input" {
        let mut def_cmd = Command::new(pactl_bin);
        #[cfg(windows)]
        def_cmd.creation_flags(0x0800_0000);
        let def_out = def_cmd
            .args(["get-default-source"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                (
                    classify_spawn_error(&e),
                    format!("Failed to resolve default input source: {e}"),
                )
            })?;
        if !def_out.status.success() {
            return Err((
                MicFeedbackKind::MissingDevice,
                "No default PipeWire/PulseAudio input source is available.".to_string(),
            ));
        }
        let name = String::from_utf8_lossy(&def_out.stdout).trim().to_string();
        if name.is_empty() || name == "@DEFAULT_SOURCE@" {
            return Err((
                MicFeedbackKind::MissingDevice,
                "No default PipeWire/PulseAudio input source is configured.".to_string(),
            ));
        }
        name
    } else if normalized == "default_output" {
        let mut def_cmd = Command::new(pactl_bin);
        #[cfg(windows)]
        def_cmd.creation_flags(0x0800_0000);
        let def_out = def_cmd
            .args(["get-default-sink"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                (
                    classify_spawn_error(&e),
                    format!("Failed to resolve default output monitor: {e}"),
                )
            })?;
        let sink = String::from_utf8_lossy(&def_out.stdout).trim().to_string();
        if !def_out.status.success() || sink.is_empty() {
            return Err((
                MicFeedbackKind::MissingDevice,
                "No default PipeWire/PulseAudio output monitor is available.".to_string(),
            ));
        }
        format!("{sink}.monitor")
    } else {
        normalized.to_string()
    };

    let matched = entries
        .into_iter()
        .find(|entry| entry.name == target_name)
        .ok_or_else(|| {
            (
                MicFeedbackKind::MissingDevice,
                format!("Selected microphone source '{source_id}' was not found."),
            )
        })?;

    let muted = query_source_mute(pactl_bin, &matched.name).await;

    Ok(ResolvedSource {
        name: matched.name,
        sample_rate: matched.sample_rate,
        channels: matched.channels,
        muted,
    })
}

async fn query_source_mute(pactl_bin: &Path, source_name: &str) -> Option<bool> {
    let mut cmd = Command::new(pactl_bin);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let out = cmd
        .args(["get-source-mute", source_name])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).to_ascii_lowercase();
    if text.contains("yes") {
        Some(true)
    } else if text.contains("no") {
        Some(false)
    } else {
        None
    }
}

pub(crate) fn parse_pactl_sources_short(stdout: &str) -> Vec<PactlSourceEntry> {
    stdout
        .lines()
        .filter_map(|line| {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() < 2 {
                return None;
            }
            let name = cols[1].to_string();
            let mut sample_rate = 48_000_u32;
            let mut channels = 1_u16;
            for token in cols.iter().skip(2) {
                if let Some(ch_str) = token.strip_suffix("ch")
                    && let Ok(ch) = ch_str.parse::<u16>()
                {
                    channels = ch.clamp(1, 2);
                } else if let Some(hz_str) = token.strip_suffix("Hz")
                    && let Ok(hz) = hz_str.parse::<u32>()
                {
                    sample_rate = hz.clamp(8_000, 192_000);
                }
            }
            Some(PactlSourceEntry {
                name,
                sample_rate,
                channels,
            })
        })
        .collect()
}

fn classify_spawn_error(err: &std::io::Error) -> MicFeedbackKind {
    match err.kind() {
        std::io::ErrorKind::PermissionDenied => MicFeedbackKind::PermissionError,
        _ => MicFeedbackKind::MissingDevice,
    }
}

fn classify_stderr_error(stderr: &str) -> MicFeedbackKind {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("permission denied")
        || lower.contains("access denied")
        || lower.contains("connection refused")
        || lower.contains("not authorized")
    {
        MicFeedbackKind::PermissionError
    } else {
        MicFeedbackKind::MissingDevice
    }
}

/// Resamples interleaved `f32` PCM from `(src_rate, src_channels)` to `(dst_rate, dst_channels)`.
pub(crate) fn resample_for_output(
    src: &[f32],
    src_rate: u32,
    src_channels: u16,
    dst_rate: u32,
    dst_channels: u16,
) -> Vec<f32> {
    if src.is_empty() || src_channels == 0 || dst_channels == 0 || src_rate == 0 || dst_rate == 0 {
        return Vec::new();
    }

    let src_ch = src_channels as usize;
    let dst_ch = dst_channels as usize;
    let src_frames = src.len() / src_ch;
    if src_frames == 0 {
        return Vec::new();
    }

    let dst_frames = ((src_frames as u128).saturating_mul(dst_rate as u128) / (src_rate as u128))
        .max(1) as usize;
    let mut out = Vec::with_capacity(dst_frames * dst_ch);

    for df in 0..dst_frames {
        let src_pos = (df as f64) * (src_rate as f64) / (dst_rate as f64);
        let idx0 = (src_pos.floor() as usize).min(src_frames - 1);
        let idx1 = (idx0 + 1).min(src_frames - 1);
        let frac = (src_pos - idx0 as f64) as f32;

        for dc in 0..dst_ch {
            let sc = if src_ch == 1 { 0 } else { dc.min(src_ch - 1) };
            let s0 = src[idx0 * src_ch + sc];
            let s1 = src[idx1 * src_ch + sc];
            out.push(s0 + (s1 - s0) * frac);
        }
    }

    out
}

fn play_samples_cpal(
    samples: &[f32],
    src_rate: u32,
    src_channels: u16,
    output_device_name: Option<&str>,
    stop_flag: Arc<AtomicBool>,
) -> std::result::Result<(), String> {
    let cursor = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let finished = Arc::new(AtomicBool::new(false));

    let (guard, _dst_rate, _dst_channels, total_samples) = start_cpal_output_stream_blocking(
        output_device_name.map(str::to_owned),
        CpalStreamSource::Buffer {
            samples: samples.to_vec(),
            src_rate,
            src_channels,
            cursor: Arc::clone(&cursor),
            finished: Arc::clone(&finished),
            error_flag: Arc::clone(&stop_flag),
        },
    )?;

    if total_samples == 0 {
        guard.stop_blocking();
        return Ok(());
    }

    while !stop_flag.load(Ordering::Relaxed)
        && !finished.load(Ordering::Relaxed)
        && cursor.load(Ordering::Relaxed) < total_samples
    {
        std::thread::sleep(Duration::from_millis(15));
    }

    guard.stop_blocking();
    Ok(())
}

pub(crate) fn write_playback_slice<T: Copy + Default>(
    out: &mut [T],
    data: &[f32],
    cursor: &std::sync::atomic::AtomicUsize,
    finished: &AtomicBool,
    convert: impl Fn(f32) -> T,
) {
    let start = cursor.load(Ordering::Relaxed);
    if start >= data.len() {
        out.fill(T::default());
        finished.store(true, Ordering::Relaxed);
        return;
    }
    let avail = (data.len() - start).min(out.len());
    for (dst, &src) in out[..avail].iter_mut().zip(&data[start..start + avail]) {
        *dst = convert(src);
    }
    if avail < out.len() {
        out[avail..].fill(T::default());
        finished.store(true, Ordering::Relaxed);
    }
    cursor.store(start + avail, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dbfs_math_computes_silence_full_scale_and_half_scale() {
        let (rms_empty, peak_empty) = compute_dbfs(&[]);
        assert_eq!(rms_empty, DBFS_FLOOR);
        assert_eq!(peak_empty, DBFS_FLOOR);

        let (rms_zero, peak_zero) = compute_dbfs(&[0.0; 128]);
        assert_eq!(rms_zero, DBFS_FLOOR);
        assert_eq!(peak_zero, DBFS_FLOOR);

        let (rms_full, peak_full) = compute_dbfs(&[1.0, -1.0, 1.0, -1.0]);
        assert!((rms_full - 0.0).abs() < 0.01);
        assert!((peak_full - 0.0).abs() < 0.01);

        let (rms_half, peak_half) = compute_dbfs(&[0.5, -0.5, 0.5, -0.5]);
        assert!((rms_half - (-6.02)).abs() < 0.1);
        assert!((peak_half - (-6.02)).abs() < 0.1);
    }

    #[test]
    fn classify_mic_signal_covers_all_feedback_states() {
        assert_eq!(
            classify_mic_signal(-12.0, -0.5, Some(false)).0,
            MicFeedbackKind::Clipping
        );
        assert_eq!(
            classify_mic_signal(-96.0, -96.0, Some(false)).0,
            MicFeedbackKind::Silence
        );
        assert_eq!(
            classify_mic_signal(-96.0, -96.0, Some(true)).0,
            MicFeedbackKind::MutedOrVeryQuiet
        );
        assert_eq!(
            classify_mic_signal(-55.0, -40.0, Some(false)).0,
            MicFeedbackKind::MutedOrVeryQuiet
        );
        assert_eq!(
            classify_mic_signal(-22.0, -10.0, Some(false)).0,
            MicFeedbackKind::GoodSignal
        );
    }

    #[test]
    fn parse_pactl_sources_short_extracts_name_rate_and_channels() {
        let sample = "\
64\talsa_input.usb-Logitech_PRO_X-00.mono-fallback\tPipeWire\ts16le 1ch 48000Hz\tRUNNING
65\talsa_input.usb-C920.analog-stereo\tPipeWire\ts16le 2ch 32000Hz\tSUSPENDED
90\teasyeffects_source\tPipeWire\tfloat32le 2ch 48000Hz\tRUNNING
";
        let entries = parse_pactl_sources_short(sample);
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries[0],
            PactlSourceEntry {
                name: "alsa_input.usb-Logitech_PRO_X-00.mono-fallback".into(),
                sample_rate: 48_000,
                channels: 1,
            }
        );
        assert_eq!(
            entries[1],
            PactlSourceEntry {
                name: "alsa_input.usb-C920.analog-stereo".into(),
                sample_rate: 32_000,
                channels: 2,
            }
        );
        assert_eq!(entries[2].name, "easyeffects_source");
    }

    #[test]
    fn resample_for_output_handles_mono_to_stereo_and_rate_change() {
        let mono_24k = vec![0.25_f32; 240];
        let stereo_48k = resample_for_output(&mono_24k, 24_000, 1, 48_000, 2);
        assert_eq!(stereo_48k.len(), 480 * 2);
        assert!(stereo_48k.iter().all(|&s| (s - 0.25).abs() < 1e-5));
    }
}
