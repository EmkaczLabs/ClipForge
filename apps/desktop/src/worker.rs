//! Non-blocking background Tokio worker wired to real ClipForge workspace backends.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use clipforge_capture::{CaptureBackend, GsrCaptureBackend, detect_gsr};
use clipforge_cloud_client::{HttpCloudClient, RemoteClipSummary, ShareVisibility};
use clipforge_common::{
    AppConfig, CaptureConfig, CaptureEvent, CaptureStatus, ClipFilter, ClipId, ClipMetadata,
    ClipStore, MediaProcessor, TrimRequest,
};
use clipforge_library::{DirectoryScanner, SqliteClipStore};
use clipforge_media::FfmpegMediaProcessor;
use rune::Waker;
use tokio::process::Command;
use tokio::sync::mpsc::{self as tokio_mpsc, UnboundedSender};

use crate::config::{config_file_path, save_app_config_to};

/// Detected availability of external runtime binaries on this Linux host.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DependencyStatus {
    pub gsr_available: bool,
    pub gsr_location: Option<String>,
    pub ffmpeg_available: bool,
    pub ffprobe_available: bool,
    pub probed: bool,
}

/// Commands dispatched from the Rune UI thread to the background Tokio worker.
#[derive(Debug, Clone)]
pub enum WorkerCommand {
    Initialize(AppConfig),
    ProbeDependencies,
    ScanLibrary(AppConfig),
    ToggleFavorite {
        id: ClipId,
        favorite: bool,
    },
    StartReplayBuffer(CaptureConfig),
    StartRecording(CaptureConfig),
    SaveReplay,
    StopCapture,
    TrimClip(TrimRequest),
    UploadClip {
        clip: Box<ClipMetadata>,
        visibility: ShareVisibility,
        server_url: Option<String>,
        api_token: Option<String>,
    },
    SetShareVisibility {
        share_id: String,
        visibility: ShareVisibility,
        server_url: Option<String>,
        api_token: Option<String>,
    },
    SaveConfig(AppConfig),
    OpenInPlayer(PathBuf),
}

/// Events emitted by the background Tokio worker back to the Rune UI thread.
#[derive(Debug, Clone)]
pub enum WorkerEvent {
    DependenciesProbed(DependencyStatus),
    LibraryLoaded {
        clips: Vec<ClipMetadata>,
        favorites: HashSet<ClipId>,
    },
    CaptureStatusChanged(CaptureStatus),
    CaptureEventReceived(CaptureEvent),
    TrimCompleted(std::result::Result<PathBuf, String>),
    UploadCompleted {
        clip_id: ClipId,
        result: std::result::Result<RemoteClipSummary, String>,
    },
    VisibilityUpdated {
        share_id: String,
        result: std::result::Result<RemoteClipSummary, String>,
    },
    ConfigSaved(std::result::Result<PathBuf, String>),
    Banner(String),
}

/// Handle held by [`crate::state::AppState`] to communicate with the background worker.
pub struct WorkerHandle {
    cmd_tx: UnboundedSender<WorkerCommand>,
    evt_rx: Arc<Mutex<Receiver<WorkerEvent>>>,
}

struct WorkerServices {
    capture: Arc<GsrCaptureBackend>,
    media: Arc<FfmpegMediaProcessor>,
    store: Arc<Mutex<Option<Arc<SqliteClipStore>>>>,
}

impl WorkerHandle {
    /// Spawns the background Tokio runtime thread and wires `waker` to wake the Rune event loop.
    pub fn spawn(waker: Waker) -> Self {
        let (cmd_tx, mut cmd_rx) = tokio_mpsc::unbounded_channel::<WorkerCommand>();
        let (evt_tx, evt_rx) = mpsc::channel::<WorkerEvent>();

        let _ = std::thread::Builder::new()
            .name("clipforge-worker".to_string())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(err) => {
                        tracing::error!(%err, "Failed to start Tokio worker runtime");
                        return;
                    }
                };

                rt.block_on(async move {
                    let capture = Arc::new(GsrCaptureBackend::new());
                    let media = Arc::new(FfmpegMediaProcessor::new());
                    let services = Arc::new(WorkerServices {
                        capture: capture.clone(),
                        media,
                        store: Arc::new(Mutex::new(None)),
                    });

                    // Forward asynchronous CaptureEvents from GsrCaptureBackend to the UI.
                    let mut cap_rx = capture.subscribe();
                    let cap_evt_tx = evt_tx.clone();
                    let cap_waker = waker.clone();
                    tokio::spawn(async move {
                        while let Ok(ev) = cap_rx.recv().await {
                            let _ = cap_evt_tx.send(WorkerEvent::CaptureEventReceived(ev));
                            (cap_waker)();
                        }
                    });

                    while let Some(cmd) = cmd_rx.recv().await {
                        let svc = services.clone();
                        let tx = evt_tx.clone();
                        let wk = waker.clone();
                        tokio::spawn(async move {
                            handle_command(cmd, svc, &tx, &wk).await;
                        });
                    }
                });
            });

        Self {
            cmd_tx,
            evt_rx: Arc::new(Mutex::new(evt_rx)),
        }
    }

    /// Sends a command to the background worker without blocking the UI thread.
    pub fn send(&self, cmd: WorkerCommand) {
        if let Err(err) = self.cmd_tx.send(cmd) {
            tracing::warn!(%err, "Worker command channel closed");
        }
    }

    /// Drains the next pending event if available without blocking.
    pub fn try_recv(&self) -> Option<WorkerEvent> {
        self.evt_rx.lock().ok()?.try_recv().ok()
    }
}

async fn handle_command(
    cmd: WorkerCommand,
    svc: Arc<WorkerServices>,
    evt_tx: &Sender<WorkerEvent>,
    waker: &Waker,
) {
    match cmd {
        WorkerCommand::Initialize(cfg) => {
            let t_init = std::time::Instant::now();
            let deps = probe_dependencies().await;
            let _ = evt_tx.send(WorkerEvent::DependenciesProbed(deps));
            let t_deps = t_init.elapsed();

            if let Some(store) = ensure_store(&svc, &cfg) {
                scan_and_load_library(&store, &svc.media, &cfg, evt_tx).await;
            }
            tracing::info!(
                deps_ms = t_deps.as_secs_f64() * 1000.0,
                total_ms = t_init.elapsed().as_secs_f64() * 1000.0,
                "Worker Initialize completed"
            );
            (waker)();
        }
        WorkerCommand::ProbeDependencies => {
            let status = probe_dependencies().await;
            let _ = evt_tx.send(WorkerEvent::DependenciesProbed(status));
            (waker)();
        }
        WorkerCommand::ScanLibrary(cfg) => {
            if let Some(store) = ensure_store(&svc, &cfg) {
                scan_and_load_library(&store, &svc.media, &cfg, evt_tx).await;
            }
            (waker)();
        }
        WorkerCommand::ToggleFavorite { id, favorite } => {
            let store = svc.store.lock().ok().and_then(|g| g.clone());
            if let Some(store) = store
                && let Err(err) = store.set_favorite(id, favorite).await
            {
                let _ = evt_tx.send(WorkerEvent::Banner(format!(
                    "Favorite update failed: {err}"
                )));
                (waker)();
            }
        }
        WorkerCommand::StartReplayBuffer(cfg) => {
            match svc.capture.start_replay_buffer(&cfg).await {
                Ok(()) => {
                    let st = svc.capture.status().await;
                    let _ = evt_tx.send(WorkerEvent::CaptureStatusChanged(st));
                }
                Err(err) => {
                    let msg = err.to_string();
                    let _ = evt_tx.send(WorkerEvent::CaptureStatusChanged(CaptureStatus::Error {
                        message: msg.clone(),
                    }));
                    let _ = evt_tx.send(WorkerEvent::Banner(msg));
                }
            }
            (waker)();
        }
        WorkerCommand::StartRecording(cfg) => {
            match svc.capture.start_recording(&cfg).await {
                Ok(()) => {
                    let st = svc.capture.status().await;
                    let _ = evt_tx.send(WorkerEvent::CaptureStatusChanged(st));
                }
                Err(err) => {
                    let msg = err.to_string();
                    let _ = evt_tx.send(WorkerEvent::CaptureStatusChanged(CaptureStatus::Error {
                        message: msg.clone(),
                    }));
                    let _ = evt_tx.send(WorkerEvent::Banner(msg));
                }
            }
            (waker)();
        }
        WorkerCommand::SaveReplay => {
            match svc.capture.save_replay().await {
                Ok(path) => {
                    let _ = evt_tx.send(WorkerEvent::Banner(format!(
                        "Replay saved: {}",
                        path.display()
                    )));
                    let store = svc.store.lock().ok().and_then(|g| g.clone());
                    if let Some(store) = store {
                        let _ = index_single_clip(&store, &svc.media, &path, true).await;
                        emit_library_loaded(&store, evt_tx).await;
                    }
                }
                Err(err) => {
                    let _ = evt_tx.send(WorkerEvent::Banner(err.to_string()));
                }
            }
            (waker)();
        }
        WorkerCommand::StopCapture => {
            match svc.capture.stop().await {
                Ok(()) => {
                    let st = svc.capture.status().await;
                    let _ = evt_tx.send(WorkerEvent::CaptureStatusChanged(st));
                }
                Err(err) => {
                    let _ = evt_tx.send(WorkerEvent::Banner(err.to_string()));
                }
            }
            (waker)();
        }
        WorkerCommand::TrimClip(req) => {
            let res = svc.media.trim(&req).await.map_err(|e| e.to_string());
            let _ = evt_tx.send(WorkerEvent::TrimCompleted(res));
            (waker)();
        }
        WorkerCommand::UploadClip {
            clip,
            visibility,
            server_url,
            api_token,
        } => {
            let clip_id = clip.id;
            let Some(url) = server_url.filter(|u| !u.trim().is_empty()) else {
                let _ = evt_tx.send(WorkerEvent::UploadCompleted {
                    clip_id,
                    result: Err("Server URL is not configured in Settings".to_string()),
                });
                (waker)();
                return;
            };
            let client = HttpCloudClient::new(url, api_token);
            let res = client
                .upload_clip_with_visibility(&clip, &clip.file_path, visibility)
                .await
                .map_err(|e| e.to_string());
            let _ = evt_tx.send(WorkerEvent::UploadCompleted {
                clip_id,
                result: res,
            });
            (waker)();
        }
        WorkerCommand::SetShareVisibility {
            share_id,
            visibility,
            server_url,
            api_token,
        } => {
            let Some(url) = server_url.filter(|u| !u.trim().is_empty()) else {
                let _ = evt_tx.send(WorkerEvent::VisibilityUpdated {
                    share_id,
                    result: Err("Server URL is not configured in Settings".to_string()),
                });
                (waker)();
                return;
            };
            let client = HttpCloudClient::new(url, api_token);
            let res = client
                .set_visibility(&share_id, visibility)
                .await
                .map_err(|e| e.to_string());
            let _ = evt_tx.send(WorkerEvent::VisibilityUpdated {
                share_id,
                result: res,
            });
            (waker)();
        }
        WorkerCommand::SaveConfig(cfg) => {
            let path = config_file_path();
            let res = save_app_config_to(&path, &cfg)
                .map(|()| path)
                .map_err(|e| e.to_string());
            let _ = evt_tx.send(WorkerEvent::ConfigSaved(res));
            (waker)();
        }
        WorkerCommand::OpenInPlayer(path) => {
            if !path.is_file() {
                let _ = evt_tx.send(WorkerEvent::Banner(format!(
                    "Clip file not found on disk: {}",
                    path.display()
                )));
                (waker)();
                return;
            }
            match Command::new("xdg-open").arg(&path).spawn() {
                Ok(_) => {
                    let _ = evt_tx.send(WorkerEvent::Banner(format!(
                        "Opened {} in system player",
                        path.display()
                    )));
                }
                Err(err) => {
                    let _ =
                        evt_tx.send(WorkerEvent::Banner(format!("Failed to open player: {err}")));
                }
            }
            (waker)();
        }
    }
}

fn ensure_store(svc: &WorkerServices, cfg: &AppConfig) -> Option<Arc<SqliteClipStore>> {
    let mut guard = svc.store.lock().ok()?;
    if let Some(existing) = guard.as_ref() {
        return Some(existing.clone());
    }
    if let Some(parent) = cfg.database_path.parent()
        && !parent.as_os_str().is_empty()
    {
        let _ = std::fs::create_dir_all(parent);
    }
    match SqliteClipStore::open(&cfg.database_path) {
        Ok(store) => {
            let arc = Arc::new(store);
            *guard = Some(arc.clone());
            Some(arc)
        }
        Err(err) => {
            tracing::error!(?cfg.database_path, %err, "Failed to open SqliteClipStore");
            None
        }
    }
}

async fn index_single_clip(
    store: &SqliteClipStore,
    media: &FfmpegMediaProcessor,
    path: &std::path::Path,
    auto_generate_thumbnails: bool,
) -> bool {
    if store.get_by_file_path(path).await.ok().flatten().is_some() {
        return false;
    }
    let Ok(probe) = media.probe(path).await else {
        return false;
    };
    let id = ClipId::new();
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Clip")
        .to_string();
    let thumb_path = if auto_generate_thumbnails {
        let thumbs_dir = path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(".thumbs");
        let t_path = thumbs_dir.join(format!("{id}.png"));
        let at_ms = (probe.duration_ms / 4).min(5_000);
        if media.generate_thumbnail(path, at_ms, &t_path).await.is_ok() {
            Some(t_path)
        } else {
            None
        }
    } else {
        None
    };
    let clip = ClipMetadata {
        id,
        title,
        game: None,
        file_path: path.to_path_buf(),
        thumbnail_path: thumb_path,
        created_at: Utc::now(),
        duration_ms: probe.duration_ms,
        width: probe.width,
        height: probe.height,
        fps: probe.fps,
        size_bytes: probe.size_bytes,
        codec: probe.video_codec,
        audio_tracks: probe.audio_tracks,
        tags: vec![],
        cloud_share: None,
        notes: None,
    };
    store.upsert_clip(&clip).await.is_ok()
}

async fn emit_library_loaded(store: &SqliteClipStore, evt_tx: &Sender<WorkerEvent>) {
    if let Ok(clips) = store.list_clips(&ClipFilter::default()).await {
        let mut favorites = HashSet::new();
        for c in &clips {
            if store.is_favorite(c.id).await.unwrap_or(false) {
                favorites.insert(c.id);
            }
        }
        let _ = evt_tx.send(WorkerEvent::LibraryLoaded { clips, favorites });
    }
}

async fn scan_and_load_library(
    store: &SqliteClipStore,
    media: &FfmpegMediaProcessor,
    cfg: &AppConfig,
    evt_tx: &Sender<WorkerEvent>,
) {
    // Emit existing SQLite clips immediately so the UI does not wait on ffprobe/ffmpeg for new files.
    emit_library_loaded(store, evt_tx).await;

    let mut indexed_any = false;
    if cfg.clips_dir.exists()
        && let Ok(new_files) = DirectoryScanner::new()
            .scan_new_recordings(&cfg.clips_dir, store)
            .await
    {
        for path in new_files {
            if index_single_clip(store, media, &path, cfg.auto_generate_thumbnails).await {
                indexed_any = true;
            }
        }
    }

    if indexed_any {
        emit_library_loaded(store, evt_tx).await;
    }
}

async fn probe_dependencies() -> DependencyStatus {
    let gsr_install = detect_gsr().await.ok();
    let gsr_location = gsr_install.map(|inst| match inst {
        clipforge_capture::GsrInstallation::Native(p) => p.display().to_string(),
        clipforge_capture::GsrInstallation::Flatpak { app_id } => {
            format!("flatpak: {app_id}")
        }
    });

    let ffmpeg_available = find_binary("ffmpeg").await.is_some();
    let ffprobe_available = find_binary("ffprobe").await.is_some();

    DependencyStatus {
        gsr_available: gsr_location.is_some(),
        gsr_location,
        ffmpeg_available,
        ffprobe_available,
        probed: true,
    }
}

async fn find_binary(name: &str) -> Option<String> {
    let output = Command::new("which").arg(name).output().await.ok()?;
    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}
