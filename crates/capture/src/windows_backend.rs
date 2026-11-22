use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
#[cfg(not(target_os = "windows"))]
use clipforge_common::ClipForgeError;
use clipforge_common::{CaptureBackend, CaptureConfig, CaptureEvent, CaptureStatus, Result};
use tokio::sync::{Mutex, broadcast};

use crate::audio::{is_mic_source, parse_routed_audio_source};

/// Resolved WASAPI loopback & microphone device selection from `CaptureConfig::audio_sources`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowsAudioSelection {
    /// `Some(None)` for default loopback, `Some(Some(id))` for a specific render device, `None` for disabled.
    pub loopback_device: Option<Option<String>>,
    /// `Some(None)` for default mic, `Some(Some(id))` for a specific capture device, `None` for disabled.
    pub mic_device: Option<Option<String>>,
}

/// Maps `CaptureConfig::audio_sources` to Windows WASAPI loopback and microphone selections.
pub fn select_windows_audio_devices(audio_sources: &[String]) -> WindowsAudioSelection {
    let mut selection = WindowsAudioSelection::default();
    for raw in audio_sources {
        let Ok((_track, source)) = parse_routed_audio_source(raw) else {
            continue;
        };
        for token in source.split('|').map(str::trim).filter(|s| !s.is_empty()) {
            if token == "default_output" || token.eq_ignore_ascii_case("loopback") {
                if selection.loopback_device.is_none() {
                    selection.loopback_device = Some(None);
                }
            } else if token == "default_input" || token.eq_ignore_ascii_case("mic") {
                if selection.mic_device.is_none() {
                    selection.mic_device = Some(None);
                }
            } else if is_mic_source(token) {
                selection.mic_device = Some(Some(token.to_string()));
            } else {
                selection.loopback_device = Some(Some(token.to_string()));
            }
        }
    }
    selection
}

#[cfg(target_os = "windows")]
enum ActiveWindowsMode {
    Replay(crate::windows_replay::runtime::WindowsReplaySession),
    Recording {
        encoder: crate::windows_encoder::runtime::WindowsEncoderSession,
        output_path: PathBuf,
    },
}

#[cfg(target_os = "windows")]
struct ActiveWindowsSession {
    wgc: crate::windows_wgc::WgcCaptureHandle,
    loopback: Option<crate::windows_audio::WasapiCaptureHandle>,
    mic: Option<crate::windows_audio::WasapiCaptureHandle>,
    output_dir: PathBuf,
    mode: ActiveWindowsMode,
}

/// [`CaptureBackend`] implementation for Windows using WGC + WASAPI + FFmpeg.
pub struct WindowsCaptureBackend {
    status: Arc<Mutex<CaptureStatus>>,
    #[cfg(target_os = "windows")]
    session: Arc<Mutex<Option<ActiveWindowsSession>>>,
    events: broadcast::Sender<CaptureEvent>,
}

impl Default for WindowsCaptureBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsCaptureBackend {
    /// Creates a new [`WindowsCaptureBackend`].
    pub fn new() -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            status: Arc::new(Mutex::new(CaptureStatus::Idle)),
            #[cfg(target_os = "windows")]
            session: Arc::new(Mutex::new(None)),
            events,
        }
    }

    async fn set_status(&self, new_status: CaptureStatus) {
        *self.status.lock().await = new_status.clone();
        let _ = self.events.send(CaptureEvent::StatusChanged(new_status));
    }
}

#[cfg(target_os = "windows")]
mod win_impl {
    use chrono::Utc;
    use clipforge_common::{
        CaptureConfig, CaptureEvent, CaptureStatus, CaptureTarget, ClipForgeError, Result,
    };
    use tokio::sync::mpsc;

    use super::{
        ActiveWindowsMode, ActiveWindowsSession, WindowsCaptureBackend,
        select_windows_audio_devices,
    };
    use crate::args::validate_config;
    use crate::windows_audio::{CapturedAudioChunk, WasapiCaptureHandle};
    use crate::windows_encoder::WindowsOutputMode;
    use crate::windows_encoder::find_ffmpeg_binary;
    use crate::windows_encoder::runtime::WindowsEncoderSession;
    use crate::windows_replay::container_extension;
    use crate::windows_replay::runtime::WindowsReplaySession;
    use crate::windows_wgc::{CapturedVideoFrame, WgcCaptureHandle};

    impl WindowsCaptureBackend {
        pub(super) async fn is_available_impl(&self) -> bool {
            find_ffmpeg_binary().is_some()
        }

        async fn start_wgc_and_wasapi(
            config: &CaptureConfig,
        ) -> Result<(
            WgcCaptureHandle,
            mpsc::Receiver<CapturedVideoFrame>,
            Option<WasapiCaptureHandle>,
            Option<mpsc::Receiver<CapturedAudioChunk>>,
            Option<WasapiCaptureHandle>,
            Option<mpsc::Receiver<CapturedAudioChunk>>,
        )> {
            let fps = config.fps.max(1);
            let (video_tx, video_rx) = mpsc::channel(8);
            let wgc = match &config.capture_target {
                CaptureTarget::FocusedWindow => {
                    WgcCaptureHandle::start_focused_window(fps, video_tx)?
                }
                CaptureTarget::Monitor(name) => {
                    WgcCaptureHandle::start_monitor(Some(name.as_str()), fps, video_tx)?
                }
                CaptureTarget::PrimaryScreen
                | CaptureTarget::Portal
                | CaptureTarget::Region { .. } => {
                    WgcCaptureHandle::start_monitor(None, fps, video_tx)?
                }
            };

            let audio_sel = select_windows_audio_devices(&config.audio_sources);

            let (loopback, loopback_rx) = match audio_sel.loopback_device {
                Some(dev_id) => {
                    let (tx, rx) = mpsc::channel(64);
                    let handle = WasapiCaptureHandle::start_loopback(dev_id.as_deref(), tx)?;
                    (Some(handle), Some(rx))
                }
                None => (None, None),
            };

            let (mic, mic_rx) = match audio_sel.mic_device {
                Some(dev_id) => {
                    let (tx, rx) = mpsc::channel(64);
                    let handle = WasapiCaptureHandle::start_mic(dev_id.as_deref(), tx)?;
                    (Some(handle), Some(rx))
                }
                None => (None, None),
            };

            Ok((wgc, video_rx, loopback, loopback_rx, mic, mic_rx))
        }

        pub(super) async fn start_replay_buffer_impl(&self, config: &CaptureConfig) -> Result<()> {
            validate_config(config)?;
            if find_ffmpeg_binary().is_none() {
                return Err(ClipForgeError::CaptureUnavailable(
                    "ffmpeg.exe was not found in the application directory or PATH".to_string(),
                ));
            }

            let mut guard = self.session.lock().await;
            if guard.is_some() {
                return Err(ClipForgeError::CaptureFailed(
                    "a capture session is already active".to_string(),
                ));
            }

            tokio::fs::create_dir_all(&config.output_dir).await?;

            let (wgc, video_rx, loopback, loopback_rx, mic, mic_rx) =
                Self::start_wgc_and_wasapi(config).await?;
            let (width, height) = wgc.dimensions();
            if width == 0 || height == 0 {
                return Err(ClipForgeError::CaptureFailed(
                    "WGC reported zero capture dimensions".to_string(),
                ));
            }

            let replay =
                WindowsReplaySession::start(config, width, height, video_rx, loopback_rx, mic_rx)
                    .await?;

            *guard = Some(ActiveWindowsSession {
                wgc,
                loopback,
                mic,
                output_dir: config.output_dir.clone(),
                mode: ActiveWindowsMode::Replay(replay),
            });
            drop(guard);

            self.set_status(CaptureStatus::ReplayBuffering {
                started_at: Utc::now(),
                buffer_secs: config.replay_buffer_secs,
            })
            .await;

            Ok(())
        }

        pub(super) async fn save_replay_impl(&self) -> Result<std::path::PathBuf> {
            let guard = self.session.lock().await;
            let active = guard.as_ref().ok_or_else(|| {
                ClipForgeError::CaptureFailed("no active replay buffer session".to_string())
            })?;
            let ActiveWindowsMode::Replay(ref replay) = active.mode else {
                return Err(ClipForgeError::CaptureFailed(
                    "current session is a standard recording, not replay buffer".to_string(),
                ));
            };

            let saved_path = replay.save_replay(&active.output_dir).await?;
            drop(guard);

            let _ = self.events.send(CaptureEvent::ReplaySaved {
                path: saved_path.clone(),
                saved_at: Utc::now(),
            });
            Ok(saved_path)
        }

        pub(super) async fn start_recording_impl(&self, config: &CaptureConfig) -> Result<()> {
            validate_config(config)?;
            if find_ffmpeg_binary().is_none() {
                return Err(ClipForgeError::CaptureUnavailable(
                    "ffmpeg.exe was not found in the application directory or PATH".to_string(),
                ));
            }

            let mut guard = self.session.lock().await;
            if guard.is_some() {
                return Err(ClipForgeError::CaptureFailed(
                    "a capture session is already active".to_string(),
                ));
            }

            tokio::fs::create_dir_all(&config.output_dir).await?;
            let ext = container_extension(config.container);
            let stamp = Utc::now().format("%Y-%m-%d_%H-%M-%S");
            let output_path = config.output_dir.join(format!("Recording_{stamp}.{ext}"));

            let (wgc, video_rx, loopback, loopback_rx, mic, mic_rx) =
                Self::start_wgc_and_wasapi(config).await?;
            let (width, height) = wgc.dimensions();
            if width == 0 || height == 0 {
                return Err(ClipForgeError::CaptureFailed(
                    "WGC reported zero capture dimensions".to_string(),
                ));
            }

            let encoder = WindowsEncoderSession::start(
                config,
                width,
                height,
                WindowsOutputMode::SingleFile(output_path.clone()),
                video_rx,
                loopback_rx,
                mic_rx,
            )
            .await?;

            *guard = Some(ActiveWindowsSession {
                wgc,
                loopback,
                mic,
                output_dir: config.output_dir.clone(),
                mode: ActiveWindowsMode::Recording {
                    encoder,
                    output_path: output_path.clone(),
                },
            });
            drop(guard);

            self.set_status(CaptureStatus::Recording {
                started_at: Utc::now(),
                output_path,
                paused: false,
            })
            .await;

            Ok(())
        }

        pub(super) async fn stop_impl(&self) -> Result<()> {
            let active = {
                let mut guard = self.session.lock().await;
                guard.take()
            };

            let Some(mut session) = active else {
                self.set_status(CaptureStatus::Idle).await;
                return Ok(());
            };

            session.wgc.stop();
            if let Some(mut lb) = session.loopback {
                lb.stop();
            }
            if let Some(mut mic) = session.mic {
                mic.stop();
            }

            match session.mode {
                ActiveWindowsMode::Replay(replay) => {
                    replay.stop().await?;
                }
                ActiveWindowsMode::Recording {
                    encoder,
                    output_path,
                } => {
                    encoder.stop().await?;
                    let _ = self.events.send(CaptureEvent::RecordingFinished {
                        path: output_path,
                        finished_at: Utc::now(),
                    });
                }
            }

            self.set_status(CaptureStatus::Idle).await;
            Ok(())
        }
    }
}

#[async_trait]
impl CaptureBackend for WindowsCaptureBackend {
    async fn is_available(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            self.is_available_impl().await
        }
        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    }

    async fn start_replay_buffer(&self, config: &CaptureConfig) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            self.start_replay_buffer_impl(config).await
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = config;
            Err(ClipForgeError::CaptureUnavailable(
                "WindowsCaptureBackend is only available on Windows".to_string(),
            ))
        }
    }

    async fn save_replay(&self) -> Result<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            self.save_replay_impl().await
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err(ClipForgeError::CaptureUnavailable(
                "WindowsCaptureBackend is only available on Windows".to_string(),
            ))
        }
    }

    async fn start_recording(&self, config: &CaptureConfig) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            self.start_recording_impl(config).await
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = config;
            Err(ClipForgeError::CaptureUnavailable(
                "WindowsCaptureBackend is only available on Windows".to_string(),
            ))
        }
    }

    async fn stop(&self) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            self.stop_impl().await
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.set_status(CaptureStatus::Idle).await;
            Ok(())
        }
    }

    async fn status(&self) -> CaptureStatus {
        self.status.lock().await.clone()
    }

    fn subscribe(&self) -> broadcast::Receiver<CaptureEvent> {
        self.events.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_windows_audio_devices_maps_default_and_routed_sources() {
        let sel = select_windows_audio_devices(&[
            "default_output".to_string(),
            "t2:default_input".to_string(),
        ]);
        assert_eq!(sel.loopback_device, Some(None));
        assert_eq!(sel.mic_device, Some(None));

        let custom =
            select_windows_audio_devices(&["t1:wasapi_render_1|mic_usb_input".to_string()]);
        assert_eq!(
            custom.loopback_device,
            Some(Some("wasapi_render_1".to_string()))
        );
        assert_eq!(custom.mic_device, Some(Some("mic_usb_input".to_string())));
    }

    #[tokio::test]
    async fn stop_when_idle_is_idempotent() {
        let backend = WindowsCaptureBackend::new();
        assert!(backend.stop().await.is_ok());
        assert_eq!(backend.status().await, CaptureStatus::Idle);
    }

    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn non_windows_reports_unavailable_cleanly() {
        let backend = WindowsCaptureBackend::new();
        assert!(!backend.is_available().await);
        let err = backend
            .start_replay_buffer(&CaptureConfig::default())
            .await
            .unwrap_err();
        assert!(matches!(err, ClipForgeError::CaptureUnavailable(_)));
    }
}
