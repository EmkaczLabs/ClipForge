use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use clipforge_common::{
    CaptureBackend, CaptureConfig, CaptureEvent, CaptureStatus, ClipForgeError, Result,
};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::sync::{Mutex, broadcast, oneshot};
use tracing::{debug, error, info, warn};

use crate::args::{build_recording_args, build_replay_args, container_flag};
use crate::audio::resolve_audio_routing;
use crate::discovery::{GsrInstallation, detect_gsr};

const SAVE_REPLAY_TIMEOUT: Duration = Duration::from_secs(15);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct ActiveProcess {
    child: Child,
    pid: u32,
    is_replay: bool,
    output_dir: PathBuf,
    output_file: Option<PathBuf>,
    save_waiters: Vec<oneshot::Sender<PathBuf>>,
    /// Keeps the temporary `-sc` script directory alive while GSR runs.
    _script_dir: Option<tempfile::TempDir>,
}

/// [`CaptureBackend`] implementation backed by `gpu-screen-recorder`.
pub struct GsrCaptureBackend {
    status: Arc<Mutex<CaptureStatus>>,
    process: Arc<Mutex<Option<ActiveProcess>>>,
    events: broadcast::Sender<CaptureEvent>,
    /// Optional override for the GSR installation (useful for testing).
    install_override: Option<GsrInstallation>,
}

impl Default for GsrCaptureBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl GsrCaptureBackend {
    /// Creates a new [`GsrCaptureBackend`] that auto-detects `gpu-screen-recorder`.
    pub fn new() -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            status: Arc::new(Mutex::new(CaptureStatus::Idle)),
            process: Arc::new(Mutex::new(None)),
            events,
            install_override: None,
        }
    }

    /// Creates a backend pinned to a specific [`GsrInstallation`] (useful in tests).
    pub fn with_installation(install: GsrInstallation) -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            status: Arc::new(Mutex::new(CaptureStatus::Idle)),
            process: Arc::new(Mutex::new(None)),
            events,
            install_override: Some(install),
        }
    }

    async fn resolve_install(&self) -> Result<GsrInstallation> {
        match &self.install_override {
            Some(install) => Ok(install.clone()),
            None => detect_gsr().await,
        }
    }

    async fn set_status(&self, new_status: CaptureStatus) {
        *self.status.lock().await = new_status.clone();
        let _ = self.events.send(CaptureEvent::StatusChanged(new_status));
    }

    /// Toggles pause/resume (`SIGUSR2`) when a regular recording is active.
    pub async fn toggle_pause(&self) -> Result<bool> {
        let pid = {
            let guard = self.process.lock().await;
            let active = guard.as_ref().ok_or_else(|| {
                ClipForgeError::CaptureFailed("no active recording to pause".to_string())
            })?;
            if active.is_replay {
                return Err(ClipForgeError::InvalidInput(
                    "SIGUSR2 pause/resume is not supported in replay buffer mode".to_string(),
                ));
            }
            active.pid
        };

        send_signal(pid, libc::SIGUSR2)?;

        let mut st = self.status.lock().await;
        if let CaptureStatus::Recording {
            started_at,
            output_path,
            paused,
        } = st.clone()
        {
            let next = CaptureStatus::Recording {
                started_at,
                output_path,
                paused: !paused,
            };
            *st = next.clone();
            let _ = self.events.send(CaptureEvent::StatusChanged(next));
            return Ok(!paused);
        }

        Err(ClipForgeError::CaptureFailed(
            "unexpected capture status during pause toggle".to_string(),
        ))
    }

    async fn ensure_idle(&self) -> Result<()> {
        let guard = self.process.lock().await;
        if guard.is_some() {
            return Err(ClipForgeError::CaptureFailed(
                "capture backend is already running; stop it first".to_string(),
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl CaptureBackend for GsrCaptureBackend {
    async fn is_available(&self) -> bool {
        self.resolve_install().await.is_ok()
    }

    async fn start_replay_buffer(&self, config: &CaptureConfig) -> Result<()> {
        self.ensure_idle().await?;
        let install = self.resolve_install().await?;
        tokio::fs::create_dir_all(&config.output_dir).await?;

        // Prepare a tiny notification script for `-sc` that echoes a tagged line to stdout,
        // allowing reliable path capture even if GSR's own stdout format varies across versions.
        let (script_dir, script_path) = create_save_hook_script()?;
        let args = build_replay_args(config, Some(&script_path), None)?;

        let mut cmd = install.command(&args);
        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| {
            ClipForgeError::CaptureFailed(format!("failed to spawn gpu-screen-recorder: {e}"))
        })?;

        let pid = child.id().ok_or_else(|| {
            ClipForgeError::CaptureFailed("spawned process has no PID".to_string())
        })?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        {
            let mut proc_guard = self.process.lock().await;
            *proc_guard = Some(ActiveProcess {
                child,
                pid,
                is_replay: true,
                output_dir: config.output_dir.clone(),
                output_file: None,
                save_waiters: Vec::new(),
                _script_dir: Some(script_dir),
            });
        }

        spawn_stream_monitors(
            pid,
            true,
            stdout,
            stderr,
            Arc::clone(&self.process),
            Arc::clone(&self.status),
            self.events.clone(),
        );

        let new_status = CaptureStatus::ReplayBuffering {
            started_at: Utc::now(),
            buffer_secs: config.replay_buffer_secs,
        };
        self.set_status(new_status).await;
        let routing = resolve_audio_routing(&config.audio_sources)?;
        info!(
            pid,
            buffer_secs = config.replay_buffer_secs,
            audio_args = ?routing.gsr_audio_args,
            includes_system = routing.includes_system,
            includes_mic = routing.includes_mic,
            "GSR replay buffer started"
        );
        Ok(())
    }

    async fn save_replay(&self) -> Result<PathBuf> {
        let (pid, rx, output_dir, snapshot_before) = {
            let mut guard = self.process.lock().await;
            let active = guard.as_mut().ok_or_else(|| {
                ClipForgeError::CaptureFailed("replay buffer is not running".to_string())
            })?;
            if !active.is_replay {
                return Err(ClipForgeError::InvalidInput(
                    "save_replay is only valid in replay buffer mode".to_string(),
                ));
            }
            let (tx, rx) = oneshot::channel();
            active.save_waiters.push(tx);
            let before = snapshot_dir_files(&active.output_dir).await;
            (active.pid, rx, active.output_dir.clone(), before)
        };

        send_signal(pid, libc::SIGUSR1)?;
        info!(pid, "sent SIGUSR1 to GSR to save replay");

        match tokio::time::timeout(SAVE_REPLAY_TIMEOUT, rx).await {
            Ok(Ok(path)) => Ok(path),
            Ok(Err(_)) => {
                if let Some(found) = find_new_file(&output_dir, &snapshot_before).await {
                    let _ = self.events.send(CaptureEvent::ReplaySaved {
                        path: found.clone(),
                        saved_at: Utc::now(),
                    });
                    return Ok(found);
                }
                let err_msg = match self.status.lock().await.clone() {
                    CaptureStatus::Error { message } => message,
                    _ => "gpu-screen-recorder exited before saving replay".to_string(),
                };
                Err(ClipForgeError::CaptureFailed(err_msg))
            }
            Err(_) => {
                // Fallback: check if a new file appeared in `output_dir`.
                if let Some(found) = find_new_file(&output_dir, &snapshot_before).await {
                    let _ = self.events.send(CaptureEvent::ReplaySaved {
                        path: found.clone(),
                        saved_at: Utc::now(),
                    });
                    return Ok(found);
                }
                Err(ClipForgeError::CaptureFailed(
                    "timed out waiting for gpu-screen-recorder to write saved replay".to_string(),
                ))
            }
        }
    }

    async fn start_recording(&self, config: &CaptureConfig) -> Result<()> {
        self.ensure_idle().await?;
        let install = self.resolve_install().await?;
        tokio::fs::create_dir_all(&config.output_dir).await?;

        let ext = container_flag(config.container);
        let filename = format!("recording_{}.{ext}", Utc::now().format("%Y%m%d_%H%M%S"));
        let output_file = config.output_dir.join(filename);
        let args = build_recording_args(config, &output_file)?;

        let mut cmd = install.command(&args);
        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| {
            ClipForgeError::CaptureFailed(format!("failed to spawn gpu-screen-recorder: {e}"))
        })?;

        let pid = child.id().ok_or_else(|| {
            ClipForgeError::CaptureFailed("spawned process has no PID".to_string())
        })?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        {
            let mut proc_guard = self.process.lock().await;
            *proc_guard = Some(ActiveProcess {
                child,
                pid,
                is_replay: false,
                output_dir: config.output_dir.clone(),
                output_file: Some(output_file.clone()),
                save_waiters: Vec::new(),
                _script_dir: None,
            });
        }

        spawn_stream_monitors(
            pid,
            false,
            stdout,
            stderr,
            Arc::clone(&self.process),
            Arc::clone(&self.status),
            self.events.clone(),
        );

        let new_status = CaptureStatus::Recording {
            started_at: Utc::now(),
            output_path: output_file.clone(),
            paused: false,
        };
        self.set_status(new_status).await;
        let routing = resolve_audio_routing(&config.audio_sources)?;
        info!(
            pid,
            path = %output_file.display(),
            audio_args = ?routing.gsr_audio_args,
            includes_system = routing.includes_system,
            includes_mic = routing.includes_mic,
            "GSR recording started"
        );
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        let mut active = {
            let mut guard = self.process.lock().await;
            match guard.take() {
                Some(proc) => proc,
                None => {
                    self.set_status(CaptureStatus::Idle).await;
                    return Ok(());
                }
            }
        };

        if let Err(err) = send_signal(active.pid, libc::SIGINT) {
            warn!(pid = active.pid, error = %err, "failed to send SIGINT; attempting kill");
            let _ = active.child.kill().await;
        } else {
            match tokio::time::timeout(SHUTDOWN_TIMEOUT, active.child.wait()).await {
                Ok(Ok(status)) => debug!(pid = active.pid, ?status, "GSR exited cleanly"),
                Ok(Err(e)) => warn!(pid = active.pid, error = %e, "error waiting for GSR exit"),
                Err(_) => {
                    warn!(
                        pid = active.pid,
                        "GSR did not exit on SIGINT; sending SIGKILL"
                    );
                    let _ = active.child.kill().await;
                    let _ = active.child.wait().await;
                }
            }
        }

        if !active.is_replay
            && let Some(path) = active.output_file
        {
            let _ = self.events.send(CaptureEvent::RecordingFinished {
                path,
                finished_at: Utc::now(),
            });
        }

        self.set_status(CaptureStatus::Idle).await;
        Ok(())
    }

    async fn status(&self) -> CaptureStatus {
        self.status.lock().await.clone()
    }

    fn subscribe(&self) -> broadcast::Receiver<CaptureEvent> {
        self.events.subscribe()
    }
}

/// Sends a POSIX signal exclusively to `pid` (never broadcasts to other GSR instances).
pub(crate) fn send_signal(pid: u32, signal: i32) -> Result<()> {
    let raw_pid = i32::try_from(pid)
        .map_err(|_| ClipForgeError::CaptureFailed(format!("PID {pid} overflows i32")))?;
    // SAFETY: `libc::kill` with a specific positive `pid_t` sends the signal only to that process.
    let rc = unsafe { libc::kill(raw_pid, signal) };
    if rc != 0 {
        return Err(ClipForgeError::Io(std::io::Error::last_os_error()));
    }
    Ok(())
}

fn create_save_hook_script() -> Result<(tempfile::TempDir, PathBuf)> {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir()?;
    let script_path = dir.path().join("on_save.sh");
    let contents = "#!/bin/sh\nprintf 'CLIPFORGE_SAVED:%s\\n' \"$1\"\n";
    std::fs::write(&script_path, contents)?;
    let mut perms = std::fs::metadata(&script_path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script_path, perms)?;
    Ok((dir, script_path))
}

/// Parses a stdout line from GSR or our `-sc` hook into a saved clip [`PathBuf`].
#[cfg(test)]
pub(crate) fn parse_saved_path_line(line: &str) -> Option<PathBuf> {
    parse_saved_path_line_with_hook(line, false)
}

fn parse_saved_path_line_with_hook(line: &str, hook_only: bool) -> Option<PathBuf> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("CLIPFORGE_SAVED:") {
        let p = rest.trim();
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    if !hook_only
        && trimmed.starts_with('/')
        && (trimmed.ends_with(".mp4")
            || trimmed.ends_with(".mkv")
            || trimmed.ends_with(".webm")
            || trimmed.ends_with(".flv"))
    {
        return Some(PathBuf::from(trimmed));
    }
    None
}

fn spawn_stream_monitors(
    pid: u32,
    has_save_hook: bool,
    stdout: Option<tokio::process::ChildStdout>,
    stderr: Option<tokio::process::ChildStderr>,
    process: Arc<Mutex<Option<ActiveProcess>>>,
    status: Arc<Mutex<CaptureStatus>>,
    events: broadcast::Sender<CaptureEvent>,
) {
    let last_stderr_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    if let Some(out) = stdout {
        let proc_clone = Arc::clone(&process);
        let events_clone = events.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                debug!(pid, line = %line, "gsr stdout");
                // When `-sc` is active, GSR prints both the raw path and the hook's
                // `CLIPFORGE_SAVED:` line; match only the hook line to avoid duplicate events.
                if let Some(path) = parse_saved_path_line_with_hook(&line, has_save_hook) {
                    let _ = events_clone.send(CaptureEvent::ReplaySaved {
                        path: path.clone(),
                        saved_at: Utc::now(),
                    });
                    let mut guard = proc_clone.lock().await;
                    if let Some(active) = guard.as_mut()
                        && active.pid == pid
                    {
                        for waiter in active.save_waiters.drain(..) {
                            let _ = waiter.send(path.clone());
                        }
                    }
                }
            }
        });
    }

    if let Some(err_stream) = stderr {
        let events_clone = events.clone();
        let err_clone = Arc::clone(&last_stderr_error);
        tokio::spawn(async move {
            let mut lines = BufReader::new(err_stream).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    warn!(pid, line = %trimmed, "gsr stderr");
                    if trimmed.to_ascii_lowercase().contains("error") {
                        *err_clone.lock().await = Some(trimmed.to_string());
                        let _ = events_clone.send(CaptureEvent::Warning(trimmed.to_string()));
                    }
                }
            }
        });
    }

    // Crash / unexpected exit monitor
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let mut guard = process.lock().await;
            let Some(active) = guard.as_mut() else {
                break;
            };
            if active.pid != pid {
                break;
            }
            match active.child.try_wait() {
                Ok(Some(exit_status)) => {
                    *guard = None;
                    drop(guard);
                    // Yield briefly so the stderr reader can flush any final error line.
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    let detail = last_stderr_error.lock().await.clone();
                    let msg = match detail {
                        Some(err_line) => format!(
                            "gpu-screen-recorder exited unexpectedly ({exit_status}): {err_line}"
                        ),
                        None => format!("gpu-screen-recorder exited unexpectedly ({exit_status})"),
                    };
                    error!(pid, %msg);
                    let err_status = CaptureStatus::Error {
                        message: msg.clone(),
                    };
                    *status.lock().await = err_status.clone();
                    let _ = events.send(CaptureEvent::Error(msg));
                    let _ = events.send(CaptureEvent::StatusChanged(err_status));
                    break;
                }
                Ok(None) => {}
                Err(e) => {
                    warn!(pid, error = %e, "try_wait failed on GSR child");
                    break;
                }
            }
        }
    });
}

async fn snapshot_dir_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(mut rd) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = rd.next_entry().await {
            files.push(entry.path());
        }
    }
    files
}

async fn find_new_file(dir: &Path, before: &[PathBuf]) -> Option<PathBuf> {
    let mut rd = tokio::fs::read_dir(dir).await.ok()?;
    while let Ok(Some(entry)) = rd.next_entry().await {
        let path = entry.path();
        if !before.contains(&path)
            && tokio::fs::metadata(&path)
                .await
                .map(|m| m.is_file())
                .unwrap_or(false)
        {
            return Some(path);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_saved_path_recognizes_hook_and_raw_paths() {
        assert_eq!(
            parse_saved_path_line("CLIPFORGE_SAVED:/tmp/replays/clip1.mp4"),
            Some(PathBuf::from("/tmp/replays/clip1.mp4"))
        );
        assert_eq!(
            parse_saved_path_line("/home/user/Videos/Replay_2026.mkv"),
            Some(PathBuf::from("/home/user/Videos/Replay_2026.mkv"))
        );
        assert_eq!(parse_saved_path_line("info: encoding started"), None);
    }

    #[tokio::test]
    async fn missing_binary_returns_capture_unavailable() {
        let backend = GsrCaptureBackend::with_installation(GsrInstallation::Native(PathBuf::from(
            "/nonexistent/path/to/gpu-screen-recorder",
        )));
        // `resolve_install` succeeds because override is provided, so `start_replay_buffer` fails on spawn:
        let cfg = CaptureConfig {
            output_dir: tempfile::tempdir().unwrap().path().to_path_buf(),
            ..CaptureConfig::default()
        };
        let err = backend.start_replay_buffer(&cfg).await.unwrap_err();
        assert!(matches!(err, ClipForgeError::CaptureFailed(_)));
    }

    #[tokio::test]
    async fn save_replay_when_idle_fails_cleanly() {
        let backend = GsrCaptureBackend::new();
        assert_eq!(backend.status().await, CaptureStatus::Idle);
        let err = backend.save_replay().await.unwrap_err();
        assert!(matches!(err, ClipForgeError::CaptureFailed(_)));
    }

    #[tokio::test]
    async fn stop_when_idle_is_idempotent() {
        let backend = GsrCaptureBackend::new();
        assert!(backend.stop().await.is_ok());
        assert_eq!(backend.status().await, CaptureStatus::Idle);
    }

    #[tokio::test]
    async fn mock_gsr_replay_save_and_clean_stop() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("clips");
        let mock_bin = dir.path().join("mock-gsr.sh");

        // Mock GSR: traps USR1 to emit saved file path, traps INT to exit 0.
        let script = r#"#!/bin/sh
trap 'printf "CLIPFORGE_SAVED:%s/replay_test.mp4\n" "$OUT_DIR"' USR1
trap 'exit 0' INT
OUT_DIR=""
prev=""
for arg in "$@"; do
  if [ "$prev" = "-o" ]; then
    OUT_DIR="$arg"
  fi
  prev="$arg"
done
while true; do
  sleep 1 &
  wait $!
done
"#;
        std::fs::write(&mock_bin, script).unwrap();
        let mut perms = std::fs::metadata(&mock_bin).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&mock_bin, perms).unwrap();

        let backend = GsrCaptureBackend::with_installation(GsrInstallation::Native(mock_bin));
        let mut rx = backend.subscribe();

        let cfg = CaptureConfig {
            output_dir: out_dir.clone(),
            ..CaptureConfig::default()
        };
        backend
            .start_replay_buffer(&cfg)
            .await
            .expect("start replay");
        assert!(matches!(
            backend.status().await,
            CaptureStatus::ReplayBuffering { .. }
        ));

        // Give the mock shell script a moment to install its USR1/INT signal traps.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let saved = backend
            .save_replay()
            .await
            .expect("save replay via SIGUSR1");
        assert_eq!(saved, out_dir.join("replay_test.mp4"));

        // Ensure ReplaySaved event was broadcast
        let mut saw_saved = false;
        while let Ok(ev) = rx.try_recv() {
            if let CaptureEvent::ReplaySaved { path, .. } = ev
                && path == out_dir.join("replay_test.mp4")
            {
                saw_saved = true;
            }
        }
        assert!(saw_saved, "expected CaptureEvent::ReplaySaved");

        backend.stop().await.expect("clean stop via SIGINT");
        assert_eq!(backend.status().await, CaptureStatus::Idle);
    }

    #[tokio::test]
    async fn mock_gsr_deduplicates_raw_and_hook_paths_and_supports_repeated_saves() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("clips");
        let mock_bin = dir.path().join("mock-gsr-dup.sh");

        // Simulates real GSR behavior: prints raw path AND runs `-sc` hook (`CLIPFORGE_SAVED:`).
        let script = r#"#!/bin/sh
OUT_DIR=""
SC_PATH=""
COUNT=0
prev=""
for arg in "$@"; do
  if [ "$prev" = "-o" ]; then OUT_DIR="$arg"; fi
  if [ "$prev" = "-sc" ]; then SC_PATH="$arg"; fi
  prev="$arg"
done
on_usr1() {
  COUNT=$((COUNT + 1))
  FILE="$OUT_DIR/replay_$COUNT.mp4"
  printf 'dummy' > "$FILE"
  printf '%s\n' "$FILE"
  if [ -n "$SC_PATH" ]; then
    "$SC_PATH" "$FILE" "replay"
  fi
}
trap 'on_usr1' USR1
trap 'exit 0' INT
while true; do
  sleep 1 &
  wait $!
done
"#;
        std::fs::write(&mock_bin, script).unwrap();
        let mut perms = std::fs::metadata(&mock_bin).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&mock_bin, perms).unwrap();

        let backend = GsrCaptureBackend::with_installation(GsrInstallation::Native(mock_bin));
        let mut rx = backend.subscribe();

        let cfg = CaptureConfig {
            output_dir: out_dir.clone(),
            ..CaptureConfig::default()
        };
        backend.start_replay_buffer(&cfg).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        let first = backend.save_replay().await.expect("first save");
        let second = backend.save_replay().await.expect("second save");
        assert_eq!(first, out_dir.join("replay_1.mp4"));
        assert_eq!(second, out_dir.join("replay_2.mp4"));

        let mut saved_events = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            if let CaptureEvent::ReplaySaved { path, .. } = ev {
                saved_events.push(path);
            }
        }
        assert_eq!(
            saved_events,
            vec![out_dir.join("replay_1.mp4"), out_dir.join("replay_2.mp4")],
            "expected exactly one ReplaySaved event per save"
        );

        backend.stop().await.unwrap();
    }

    #[tokio::test]
    async fn mock_gsr_unexpected_exit_reports_stderr_and_allows_recovery() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("clips");
        let mock_bin = dir.path().join("mock-gsr-fail.sh");

        let script = r#"#!/bin/sh
echo "gsr error: simulated encoder failure" >&2
exit 2
"#;
        std::fs::write(&mock_bin, script).unwrap();
        let mut perms = std::fs::metadata(&mock_bin).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&mock_bin, perms).unwrap();

        let backend = GsrCaptureBackend::with_installation(GsrInstallation::Native(mock_bin));
        let mut rx = backend.subscribe();

        let cfg = CaptureConfig {
            output_dir: out_dir.clone(),
            ..CaptureConfig::default()
        };
        backend.start_replay_buffer(&cfg).await.unwrap();

        // Wait for the crash monitor (250ms poll) to detect the unexpected exit.
        tokio::time::sleep(Duration::from_millis(400)).await;

        let status = backend.status().await;
        match status {
            CaptureStatus::Error { ref message } => {
                assert!(
                    message.contains("simulated encoder failure"),
                    "expected stderr detail in error message, got: {message}"
                );
            }
            other => panic!("expected CaptureStatus::Error, got {other:?}"),
        }

        let mut saw_error_event = false;
        while let Ok(ev) = rx.try_recv() {
            if let CaptureEvent::Error(msg) = ev
                && msg.contains("simulated encoder failure")
            {
                saw_error_event = true;
            }
        }
        assert!(saw_error_event, "expected CaptureEvent::Error with stderr");

        // Recovery: stop resets to Idle and a new session can be started.
        backend.stop().await.unwrap();
        assert_eq!(backend.status().await, CaptureStatus::Idle);
    }

    #[tokio::test]
    #[ignore = "requires live gpu-screen-recorder and display session"]
    async fn live_gsr_replay_save_repeated_and_stop() {
        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("clips");
        let backend = GsrCaptureBackend::new();
        assert!(
            backend.is_available().await,
            "gpu-screen-recorder must be available"
        );

        let mut rx = backend.subscribe();
        let cfg = CaptureConfig {
            output_dir: out_dir.clone(),
            replay_buffer_secs: 5,
            ..CaptureConfig::default()
        };

        backend
            .start_replay_buffer(&cfg)
            .await
            .expect("start live replay buffer");
        tokio::time::sleep(Duration::from_secs(2)).await;

        let first = backend.save_replay().await.expect("first live replay save");
        assert!(first.exists(), "first replay file should exist");
        assert!(
            std::fs::metadata(&first).unwrap().len() > 0,
            "first replay file should be non-empty"
        );

        tokio::time::sleep(Duration::from_secs(2)).await;
        let second = backend
            .save_replay()
            .await
            .expect("second live replay save");
        assert!(second.exists(), "second replay file should exist");
        assert_ne!(
            first, second,
            "repeated saves should produce distinct files"
        );
        assert!(
            std::fs::metadata(&second).unwrap().len() > 0,
            "second replay file should be non-empty"
        );

        backend.stop().await.expect("clean stop via SIGINT");
        assert_eq!(backend.status().await, CaptureStatus::Idle);

        let mut saved_events = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            if let CaptureEvent::ReplaySaved { path, .. } = ev {
                saved_events.push(path);
            }
        }
        assert_eq!(saved_events, vec![first, second]);
    }
}
