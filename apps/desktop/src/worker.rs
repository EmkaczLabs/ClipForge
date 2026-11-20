//! Non-blocking background Tokio worker wired to real ClipForge workspace backends.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use clipforge_capture::{
    AudioDeviceInfo, CaptureBackend, GlobalHotkeyStatus, GsrCaptureBackend, HotkeyAction,
    HotkeyManager, HotkeySpec, MonitorInfo, detect_gsr, list_application_audio, list_audio_devices,
    list_monitors, parse_hotkey,
};
use clipforge_cloud_client::{
    CreateShareRequest, CreateShareResponse, HttpCloudClient, QuotaDto, RemoteClipSummary,
    ServerInfoDto, ShareVisibility, UserInfo,
};
use clipforge_common::{
    AppConfig, CaptureConfig, CaptureEvent, CaptureStatus, ClipFilter, ClipId, ClipMetadata,
    ClipStore, CloudClient, HotkeyBindings, MediaProcessor, TrimRequest,
};
use clipforge_library::{ClipListOptions, ClipOrigin, DirectoryScanner, SqliteClipStore};
use clipforge_media::{
    DEFAULT_MAX_DECODE_DIM, ExportCancelToken, FfmpegMediaProcessor, MicTestEvent, MicTestHandle,
    PlaybackState, PlayerEvent, VideoPlayerHandle, extract_timeline_thumbnails,
};
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
#[allow(dead_code)]
pub enum WorkerCommand {
    Initialize(AppConfig),
    ProbeDependencies,
    ScanLibrary(AppConfig),
    ToggleFavorite {
        id: ClipId,
        favorite: bool,
    },
    SetFavoriteBulk {
        ids: Vec<ClipId>,
        favorite: bool,
    },
    MoveToTrash {
        ids: Vec<ClipId>,
        managed_roots: Vec<PathBuf>,
    },
    RestoreFromTrash {
        ids: Vec<ClipId>,
    },
    PermanentlyDelete {
        ids: Vec<ClipId>,
        managed_roots: Vec<PathBuf>,
    },
    RenameClip {
        id: ClipId,
        title: String,
    },
    OpenContainingFolder(PathBuf),
    StartReplayBuffer(CaptureConfig),
    StartRecording(CaptureConfig),
    SaveReplay,
    StopCapture,
    TrimClip(TrimRequest),
    CancelTrim,
    UploadClip {
        clip: Box<ClipMetadata>,
        visibility: ShareVisibility,
        server_url: Option<String>,
        api_token: Option<String>,
    },
    CancelUpload(ClipId),
    SetShareVisibility {
        share_id: String,
        visibility: ShareVisibility,
        server_url: Option<String>,
        api_token: Option<String>,
    },
    CreateShare {
        clip_id: ClipId,
        visibility: ShareVisibility,
        expires_in_hours: Option<u64>,
        server_url: String,
        api_token: Option<String>,
    },
    RevokeShare {
        clip_id: ClipId,
        share_id: String,
        server_url: String,
        api_token: Option<String>,
    },
    SaveConfig(AppConfig),
    OpenInPlayer(PathBuf),
    LoadThumbnail {
        path: PathBuf,
        pixels: rune::Pixels,
    },
    OpenPlayer {
        clip_id: ClipId,
        path: PathBuf,
        trim_range: Option<(u64, u64)>,
        pixels: rune::Pixels,
        autoplay: bool,
        volume: f32,
        muted: bool,
    },
    ClosePlayer,
    PlayerPlay,
    PlayerPause,
    PlayerSeek(u64),
    PlayerSetTrimRange(Option<(u64, u64)>),
    PlayerSetVolume(f32),
    PlayerSetMuted(bool),
    SetAudioTrack(Option<usize>),
    DiscoverCaptureSources,
    CheckServerHealth {
        server_url: String,
    },
    ConnectServer {
        server_url: String,
        api_token: Option<String>,
        username: Option<String>,
        password: Option<String>,
        register: bool,
    },
    DisconnectServer,
    RefreshRemoteShares {
        server_url: String,
        api_token: Option<String>,
    },
    DeleteRemoteShare {
        share_id: String,
        server_url: String,
        api_token: Option<String>,
    },
    CopyToClipboard(String),
    OpenUrl(String),
    StartMicTest(String),
    StartMicMonitor(String),
    StopMicTest,
    PlayMicTestSample,
    ClearMicTest,
    RestartReplayBuffer(CaptureConfig),
}

/// Events emitted by the background Tokio worker back to the Rune UI thread.
#[derive(Debug, Clone)]
pub enum WorkerEvent {
    DependenciesProbed(DependencyStatus),
    LibraryLoaded {
        clips: Vec<ClipMetadata>,
        trashed_clips: Vec<ClipMetadata>,
        favorites: HashSet<ClipId>,
        origins: HashMap<ClipId, ClipOrigin>,
        trashed_at: HashMap<ClipId, DateTime<Utc>>,
    },
    CaptureStatusChanged(CaptureStatus),
    CaptureEventReceived(CaptureEvent),
    HotkeyStatusChanged(GlobalHotkeyStatus),
    TrimProgress(f32),
    TrimCompleted(std::result::Result<PathBuf, String>),
    UploadProgress {
        clip_id: ClipId,
        progress_pct: u8,
    },
    UploadCompleted {
        clip_id: ClipId,
        result: std::result::Result<RemoteClipSummary, String>,
    },
    UploadCancelled {
        clip_id: ClipId,
    },
    VisibilityUpdated {
        share_id: String,
        result: std::result::Result<RemoteClipSummary, String>,
    },
    ShareCreated {
        clip_id: ClipId,
        result: std::result::Result<CreateShareResponse, String>,
    },
    ShareRevoked {
        clip_id: ClipId,
        share_id: String,
        result: std::result::Result<(), String>,
    },
    ConfigSaved(std::result::Result<PathBuf, String>),
    Banner(String),
    PlayerFrameDelivered {
        clip_id: ClipId,
        width: u32,
        height: u32,
        pts_ms: u64,
    },
    PlayerStateChanged {
        clip_id: ClipId,
        playing: bool,
    },
    PlayerPosition {
        clip_id: ClipId,
        pts_ms: u64,
    },
    PlayerAudioStatus {
        clip_id: ClipId,
        has_audio: bool,
        error: Option<String>,
    },
    PlayerError {
        clip_id: ClipId,
        message: String,
    },
    TimelineThumbsLoaded {
        clip_id: ClipId,
        thumbs: Vec<rune::Pixels>,
    },
    CaptureSourcesDiscovered {
        monitors: Vec<MonitorInfo>,
        audio_devices: Vec<AudioDeviceInfo>,
        app_audio: Vec<AudioDeviceInfo>,
    },
    ServerHealthChecked {
        ok: bool,
        error: Option<String>,
        server_info: Option<ServerInfoDto>,
    },
    ServerConnected(std::result::Result<(String, Option<String>), String>),
    ServerMetadataLoaded {
        server_info: Option<ServerInfoDto>,
        current_user: Option<UserInfo>,
        quota: Option<QuotaDto>,
    },
    RemoteSharesLoaded(std::result::Result<Vec<RemoteClipSummary>, String>),
    RemoteShareDeleted {
        share_id: String,
        result: std::result::Result<(), String>,
    },
    MicTest(MicTestEvent),
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
    player: VideoPlayerHandle,
    active_player_clip: Arc<Mutex<Option<(ClipId, rune::Pixels)>>>,
    active_trim_cancel: Arc<Mutex<Option<ExportCancelToken>>>,
    active_uploads: Arc<Mutex<HashMap<ClipId, Arc<AtomicBool>>>>,
    config: Arc<Mutex<AppConfig>>,
    hotkey_manager: Arc<Mutex<Option<HotkeyManager>>>,
    hotkey_tx: UnboundedSender<HotkeyAction>,
    hotkey_status_tx: UnboundedSender<GlobalHotkeyStatus>,
    mic_test: MicTestHandle,
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
                    let (player, mut player_rx) = VideoPlayerHandle::spawn();
                    let (mic_test, mut mic_rx) = MicTestHandle::spawn();
                    let active_player_clip: Arc<Mutex<Option<(ClipId, rune::Pixels)>>> =
                        Arc::new(Mutex::new(None));
                    let (hotkey_tx, mut hotkey_rx) =
                        tokio_mpsc::unbounded_channel::<HotkeyAction>();
                    let (hotkey_status_tx, mut hotkey_status_rx) =
                        tokio_mpsc::unbounded_channel::<GlobalHotkeyStatus>();
                    let services = Arc::new(WorkerServices {
                        capture: capture.clone(),
                        media,
                        store: Arc::new(Mutex::new(None)),
                        player,
                        active_player_clip: active_player_clip.clone(),
                        active_trim_cancel: Arc::new(Mutex::new(None)),
                        active_uploads: Arc::new(Mutex::new(HashMap::new())),
                        config: Arc::new(Mutex::new(AppConfig::default())),
                        hotkey_manager: Arc::new(Mutex::new(None)),
                        hotkey_tx,
                        hotkey_status_tx,
                        mic_test,
                    });

                    // Forward MicTestEvents from MicTestHandle to the UI.
                    let mic_evt_tx = evt_tx.clone();
                    let mic_waker = waker.clone();
                    tokio::spawn(async move {
                        while let Some(ev) = mic_rx.recv().await {
                            let _ = mic_evt_tx.send(WorkerEvent::MicTest(ev));
                            (mic_waker)();
                        }
                    });

                    // Forward GlobalHotkeyStatus updates to the UI.
                    let hk_status_evt_tx = evt_tx.clone();
                    let hk_status_waker = waker.clone();
                    tokio::spawn(async move {
                        while let Some(status) = hotkey_status_rx.recv().await {
                            let _ = hk_status_evt_tx.send(WorkerEvent::HotkeyStatusChanged(status));
                            (hk_status_waker)();
                        }
                    });

                    // Dispatch triggered global hotkeys using the same capture path as UI buttons.
                    let hk_svc = services.clone();
                    let hk_evt_tx = evt_tx.clone();
                    let hk_waker = waker.clone();
                    tokio::spawn(async move {
                        while let Some(action) = hotkey_rx.recv().await {
                            handle_hotkey_action(action, &hk_svc, &hk_evt_tx, &hk_waker).await;
                        }
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

                    // Forward decoded video frames and player events off the UI thread.
                    let pl_evt_tx = evt_tx.clone();
                    let pl_waker = waker.clone();
                    tokio::spawn(async move {
                        while let Some(ev) = player_rx.recv().await {
                            let active = active_player_clip.lock().ok().and_then(|g| g.clone());
                            let Some((clip_id, pixels)) = active else {
                                continue;
                            };
                            match ev {
                                PlayerEvent::Metadata(_) => {}
                                PlayerEvent::Frame(frame) => {
                                    let width = frame.width;
                                    let height = frame.height;
                                    let pts_ms = frame.pts_ms;
                                    pixels.set(width, height, frame.data);
                                    let _ = pl_evt_tx.send(WorkerEvent::PlayerFrameDelivered {
                                        clip_id,
                                        width,
                                        height,
                                        pts_ms,
                                    });
                                    (pl_waker)();
                                }
                                PlayerEvent::StateChanged(state) => {
                                    let playing = matches!(state, PlaybackState::Playing);
                                    let _ = pl_evt_tx
                                        .send(WorkerEvent::PlayerStateChanged { clip_id, playing });
                                    (pl_waker)();
                                }
                                PlayerEvent::Position(pts_ms) => {
                                    let _ = pl_evt_tx
                                        .send(WorkerEvent::PlayerPosition { clip_id, pts_ms });
                                    (pl_waker)();
                                }
                                PlayerEvent::AudioStatus { has_audio, error } => {
                                    let _ = pl_evt_tx.send(WorkerEvent::PlayerAudioStatus {
                                        clip_id,
                                        has_audio,
                                        error,
                                    });
                                    (pl_waker)();
                                }
                                PlayerEvent::Error(message) => {
                                    let _ = pl_evt_tx
                                        .send(WorkerEvent::PlayerError { clip_id, message });
                                    (pl_waker)();
                                }
                            }
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
            if let Ok(mut guard) = svc.config.lock() {
                *guard = cfg.clone();
            }
            register_hotkeys(&svc, &cfg.hotkeys);
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
        WorkerCommand::SetFavoriteBulk { ids, favorite } => {
            let store = svc.store.lock().ok().and_then(|g| g.clone());
            if let Some(store) = store {
                let mut errs = Vec::new();
                for id in ids {
                    if let Err(err) = store.set_favorite(id, favorite).await {
                        errs.push(err.to_string());
                    }
                }
                if !errs.is_empty() {
                    let _ = evt_tx.send(WorkerEvent::Banner(format!(
                        "Favorite update failed: {}",
                        errs.join("; ")
                    )));
                }
                emit_library_loaded(&store, evt_tx).await;
                (waker)();
            }
        }
        WorkerCommand::MoveToTrash { ids, managed_roots } => {
            close_player_if_matches(&svc, &ids);
            let store = svc.store.lock().ok().and_then(|g| g.clone());
            if let Some(store) = store {
                let mut moved = 0usize;
                let mut missing_files = 0usize;
                let mut errs = Vec::new();
                for id in ids {
                    match store.move_to_trash(id, &managed_roots).await {
                        Ok(res) => {
                            moved += 1;
                            if !res.file_moved {
                                missing_files += 1;
                            }
                        }
                        Err(err) => errs.push(err.to_string()),
                    }
                }
                let msg = if errs.is_empty() {
                    if missing_files > 0 {
                        format!(
                            "Moved {moved} clip(s) to Trash ({missing_files} file(s) already missing on disk)"
                        )
                    } else {
                        format!("Moved {moved} clip(s) to Trash")
                    }
                } else {
                    format!(
                        "Moved {moved} clip(s) to Trash; {} error(s): {}",
                        errs.len(),
                        errs.join("; ")
                    )
                };
                let _ = evt_tx.send(WorkerEvent::Banner(msg));
                emit_library_loaded(&store, evt_tx).await;
                (waker)();
            }
        }
        WorkerCommand::RestoreFromTrash { ids } => {
            let store = svc.store.lock().ok().and_then(|g| g.clone());
            if let Some(store) = store {
                let mut restored = 0usize;
                let mut errs = Vec::new();
                for id in ids {
                    match store.restore_from_trash(id).await {
                        Ok(_) => restored += 1,
                        Err(err) => errs.push(err.to_string()),
                    }
                }
                let msg = if errs.is_empty() {
                    format!("Restored {restored} clip(s) from Trash")
                } else {
                    format!(
                        "Restored {restored} clip(s); {} error(s): {}",
                        errs.len(),
                        errs.join("; ")
                    )
                };
                let _ = evt_tx.send(WorkerEvent::Banner(msg));
                emit_library_loaded(&store, evt_tx).await;
                (waker)();
            }
        }
        WorkerCommand::PermanentlyDelete { ids, managed_roots } => {
            close_player_if_matches(&svc, &ids);
            let store = svc.store.lock().ok().and_then(|g| g.clone());
            if let Some(store) = store {
                let mut deleted = 0usize;
                let mut errs = Vec::new();
                for id in ids {
                    match store.permanently_delete(id, &managed_roots).await {
                        Ok(_) => deleted += 1,
                        Err(err) => errs.push(err.to_string()),
                    }
                }
                let msg = if errs.is_empty() {
                    format!("Permanently deleted {deleted} clip(s)")
                } else {
                    format!(
                        "Deleted {deleted} clip(s); {} error(s): {}",
                        errs.len(),
                        errs.join("; ")
                    )
                };
                let _ = evt_tx.send(WorkerEvent::Banner(msg));
                emit_library_loaded(&store, evt_tx).await;
                (waker)();
            }
        }
        WorkerCommand::RenameClip { id, title } => {
            let store = svc.store.lock().ok().and_then(|g| g.clone());
            if let Some(store) = store {
                match store.rename_clip(id, &title).await {
                    Ok(true) => {
                        let _ = evt_tx.send(WorkerEvent::Banner(format!(
                            "Renamed clip to \"{}\"",
                            title.trim()
                        )));
                        emit_library_loaded(&store, evt_tx).await;
                    }
                    Ok(false) => {
                        let _ = evt_tx.send(WorkerEvent::Banner(
                            "Clip not found in database".to_string(),
                        ));
                    }
                    Err(err) => {
                        let _ = evt_tx.send(WorkerEvent::Banner(format!("Rename failed: {err}")));
                    }
                }
                (waker)();
            }
        }
        WorkerCommand::OpenContainingFolder(path) => {
            let Some(parent) = path.parent().filter(|p| p.is_dir()) else {
                let _ = evt_tx.send(WorkerEvent::Banner(format!(
                    "Containing folder does not exist: {}",
                    path.display()
                )));
                (waker)();
                return;
            };
            match Command::new("xdg-open").arg(parent).spawn() {
                Ok(_) => {
                    let _ = evt_tx.send(WorkerEvent::Banner(format!(
                        "Opened folder {}",
                        parent.display()
                    )));
                }
                Err(err) => {
                    let _ =
                        evt_tx.send(WorkerEvent::Banner(format!("Failed to open folder: {err}")));
                }
            }
            (waker)();
        }
        WorkerCommand::StartReplayBuffer(cfg) => {
            if let Ok(mut guard) = svc.config.lock() {
                guard.capture = cfg.clone();
            }
            execute_start_replay_buffer(&svc, &cfg, evt_tx, waker).await;
        }
        WorkerCommand::StartRecording(cfg) => {
            if let Ok(mut guard) = svc.config.lock() {
                guard.capture = cfg.clone();
            }
            execute_start_recording(&svc, &cfg, evt_tx, waker).await;
        }
        WorkerCommand::SaveReplay => {
            execute_save_replay(&svc, evt_tx, waker).await;
        }
        WorkerCommand::StopCapture => {
            execute_stop_capture(&svc, evt_tx, waker).await;
        }
        WorkerCommand::TrimClip(req) => {
            let cancel = ExportCancelToken::new();
            if let Ok(mut guard) = svc.active_trim_cancel.lock() {
                *guard = Some(cancel.clone());
            }
            let prog_tx = evt_tx.clone();
            let prog_waker = waker.clone();
            let res = svc
                .media
                .trim_with_progress(&req, Some(&cancel), |p| {
                    let _ = prog_tx.send(WorkerEvent::TrimProgress(p));
                    (prog_waker)();
                })
                .await
                .map_err(|e| e.to_string());
            if let Ok(mut guard) = svc.active_trim_cancel.lock() {
                *guard = None;
            }
            if let Ok(ref out_path) = res
                && let Some(store) = svc.store.lock().ok().and_then(|g| g.clone())
                && index_clip_with_origin(
                    &store,
                    &svc.media,
                    out_path,
                    true,
                    Some(ClipOrigin::Trimmed),
                    true,
                )
                .await
            {
                emit_library_loaded(&store, evt_tx).await;
            }
            let _ = evt_tx.send(WorkerEvent::TrimCompleted(res));
            (waker)();
        }
        WorkerCommand::CancelTrim => {
            if let Ok(guard) = svc.active_trim_cancel.lock()
                && let Some(token) = guard.as_ref()
            {
                token.cancel();
            }
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
            let cancel_flag = Arc::new(AtomicBool::new(false));
            if let Ok(mut guard) = svc.active_uploads.lock() {
                if let Some(prev) = guard.get(&clip_id) {
                    prev.store(true, Ordering::SeqCst);
                }
                guard.insert(clip_id, cancel_flag.clone());
            }
            let active_uploads = svc.active_uploads.clone();
            let evt_tx_up = evt_tx.clone();
            let waker_up = waker.clone();
            tokio::spawn(async move {
                let client = HttpCloudClient::new(url, api_token);
                let _ = evt_tx_up.send(WorkerEvent::UploadProgress {
                    clip_id,
                    progress_pct: 1,
                });
                (waker_up)();
                let mut last_pct = 1u8;
                let res = client
                    .upload_clip_streaming(&clip, &clip.file_path, visibility, |pct| {
                        if pct != last_pct {
                            last_pct = pct;
                            let _ = evt_tx_up.send(WorkerEvent::UploadProgress {
                                clip_id,
                                progress_pct: pct,
                            });
                            (waker_up)();
                        }
                    })
                    .await
                    .map_err(|e| e.to_string());

                if let Ok(mut guard) = active_uploads.lock() {
                    guard.remove(&clip_id);
                }

                if cancel_flag.load(Ordering::SeqCst) {
                    if let Ok(summary) = &res {
                        let _ = client.delete_clip(&summary.clip_id.to_string()).await;
                    }
                    let _ = evt_tx_up.send(WorkerEvent::UploadCancelled { clip_id });
                    (waker_up)();
                    return;
                }

                if res.is_ok() {
                    let quota = client.quota().await.ok();
                    let current_user = client.current_user().await.ok();
                    let server_info = client.server_info().await.ok();
                    let _ = evt_tx_up.send(WorkerEvent::ServerMetadataLoaded {
                        server_info,
                        current_user,
                        quota,
                    });
                }

                let _ = evt_tx_up.send(WorkerEvent::UploadCompleted {
                    clip_id,
                    result: res,
                });
                (waker_up)();
            });
        }
        WorkerCommand::CancelUpload(clip_id) => {
            if let Ok(mut guard) = svc.active_uploads.lock()
                && let Some(flag) = guard.remove(&clip_id)
            {
                flag.store(true, Ordering::SeqCst);
            }
            let _ = evt_tx.send(WorkerEvent::UploadCancelled { clip_id });
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
        WorkerCommand::CreateShare {
            clip_id,
            visibility,
            expires_in_hours,
            server_url,
            api_token,
        } => {
            let client = HttpCloudClient::new(server_url, api_token);
            let req = CreateShareRequest {
                visibility,
                expires_in_hours,
            };
            let res = client
                .create_share(&clip_id.to_string(), &req)
                .await
                .map_err(|e| e.to_string());
            let _ = evt_tx.send(WorkerEvent::ShareCreated {
                clip_id,
                result: res,
            });
            (waker)();
        }
        WorkerCommand::RevokeShare {
            clip_id,
            share_id,
            server_url,
            api_token,
        } => {
            let client = HttpCloudClient::new(server_url, api_token);
            let res = client
                .revoke_share(&share_id)
                .await
                .map_err(|e| e.to_string());
            let _ = evt_tx.send(WorkerEvent::ShareRevoked {
                clip_id,
                share_id,
                result: res,
            });
            (waker)();
        }
        WorkerCommand::SaveConfig(cfg) => {
            let hotkeys_changed = svc
                .config
                .lock()
                .map(|mut guard| {
                    let changed = guard.hotkeys != cfg.hotkeys;
                    *guard = cfg.clone();
                    changed
                })
                .unwrap_or(false);
            if hotkeys_changed {
                register_hotkeys(&svc, &cfg.hotkeys);
            }
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
        WorkerCommand::LoadThumbnail { path, pixels } => {
            if let Ok(rgba) = decode_thumbnail_rgba(&path, 320, 180).await {
                pixels.set(320, 180, rgba);
                (waker)();
            }
        }
        WorkerCommand::OpenPlayer {
            clip_id,
            path,
            trim_range,
            pixels,
            autoplay,
            volume,
            muted,
        } => {
            if let Ok(mut guard) = svc.active_player_clip.lock() {
                *guard = Some((clip_id, pixels));
            }
            svc.player.set_volume(volume);
            svc.player.set_muted(muted);
            svc.player
                .open(&path, trim_range, Some(DEFAULT_MAX_DECODE_DIM));
            if autoplay {
                svc.player.play();
            }

            let thumb_tx = evt_tx.clone();
            let thumb_waker = waker.clone();
            let active_ref = svc.active_player_clip.clone();
            tokio::spawn(async move {
                if let Ok(frames) = extract_timeline_thumbnails(&path, 8, 160, 90).await {
                    let still_active = active_ref
                        .lock()
                        .ok()
                        .and_then(|g| g.as_ref().map(|(id, _)| *id))
                        == Some(clip_id);
                    if still_active {
                        let thumbs = frames
                            .into_iter()
                            .map(|f| {
                                let px = rune::Pixels::new();
                                px.set(f.width, f.height, f.data);
                                px
                            })
                            .collect();
                        let _ =
                            thumb_tx.send(WorkerEvent::TimelineThumbsLoaded { clip_id, thumbs });
                        (thumb_waker)();
                    }
                }
            });
        }
        WorkerCommand::ClosePlayer => {
            if let Ok(mut guard) = svc.active_player_clip.lock() {
                *guard = None;
            }
            svc.player.close();
        }
        WorkerCommand::PlayerPlay => {
            svc.player.play();
        }
        WorkerCommand::PlayerPause => {
            svc.player.pause();
        }
        WorkerCommand::PlayerSeek(ms) => {
            svc.player.seek(ms);
        }
        WorkerCommand::PlayerSetTrimRange(range) => {
            svc.player.set_trim_range(range);
        }
        WorkerCommand::PlayerSetVolume(volume) => {
            svc.player.set_volume(volume);
        }
        WorkerCommand::PlayerSetMuted(muted) => {
            svc.player.set_muted(muted);
        }
        WorkerCommand::SetAudioTrack(track) => {
            svc.player.set_audio_track(track);
        }
        WorkerCommand::DiscoverCaptureSources => {
            if let Ok(install) = detect_gsr().await {
                let (monitors_res, audio_res, app_res) = tokio::join!(
                    list_monitors(&install),
                    list_audio_devices(&install),
                    list_application_audio(&install)
                );
                let _ = evt_tx.send(WorkerEvent::CaptureSourcesDiscovered {
                    monitors: monitors_res.unwrap_or_default(),
                    audio_devices: audio_res.unwrap_or_default(),
                    app_audio: app_res.unwrap_or_default(),
                });
            } else {
                let _ = evt_tx.send(WorkerEvent::CaptureSourcesDiscovered {
                    monitors: Vec::new(),
                    audio_devices: Vec::new(),
                    app_audio: Vec::new(),
                });
            }
            (waker)();
        }
        WorkerCommand::CheckServerHealth { server_url } => {
            let client = HttpCloudClient::new(server_url, None);
            let res = client.health_check().await;
            let server_info = if res.is_ok() {
                client.server_info().await.ok()
            } else {
                None
            };
            let _ = evt_tx.send(WorkerEvent::ServerHealthChecked {
                ok: res.is_ok(),
                error: res.err().map(|e| e.to_string()),
                server_info,
            });
            (waker)();
        }
        WorkerCommand::ConnectServer {
            server_url,
            api_token,
            username,
            password,
            register,
        } => {
            let clean_url = server_url.trim().trim_end_matches('/').to_string();
            let result = async {
                if clean_url.is_empty() {
                    return Err("Server URL cannot be empty".to_string());
                }
                let mut client = HttpCloudClient::new(&clean_url, api_token.clone());
                client.health_check().await.map_err(|e| e.to_string())?;
                let server_info = client.server_info().await.ok();
                let final_token = if let (Some(u), Some(p)) = (username, password) {
                    let auth = if register {
                        client.register(&u, &p).await.map_err(|e| e.to_string())?
                    } else {
                        client.login(&u, &p).await.map_err(|e| e.to_string())?
                    };
                    client.set_token(Some(auth.token.clone()));
                    Some(auth.token)
                } else if api_token.is_some() {
                    if client.current_user().await.is_err() {
                        client
                            .list_clips(Some(1), Some(0))
                            .await
                            .map_err(|e| e.to_string())?;
                    }
                    api_token
                } else {
                    None
                };

                if let Some(tok) = final_token.as_deref()
                    && let Err(err) = crate::config::store_api_token(&clean_url, tok)
                {
                    tracing::warn!(%err, "Failed to persist API token to secure storage");
                }

                let current_user = if final_token.is_some() {
                    client.current_user().await.ok()
                } else {
                    None
                };
                let quota = if final_token.is_some() {
                    current_user
                        .as_ref()
                        .and_then(|u| u.quota)
                        .or(client.quota().await.ok())
                } else {
                    None
                };

                let _ = evt_tx.send(WorkerEvent::ServerMetadataLoaded {
                    server_info,
                    current_user,
                    quota,
                });

                Ok((clean_url, final_token))
            }
            .await;
            let _ = evt_tx.send(WorkerEvent::ServerConnected(result));
            (waker)();
        }
        WorkerCommand::DisconnectServer => {
            let _ = evt_tx.send(WorkerEvent::Banner(
                "Disconnected from cloud server".to_string(),
            ));
            (waker)();
        }
        WorkerCommand::RefreshRemoteShares {
            server_url,
            api_token,
        } => {
            let client = HttpCloudClient::new(server_url, api_token);
            let res = client
                .list_clips(Some(50), Some(0))
                .await
                .map_err(|e| e.to_string());
            if res.is_ok() {
                let server_info = client.server_info().await.ok();
                let current_user = client.current_user().await.ok();
                let quota = current_user
                    .as_ref()
                    .and_then(|u| u.quota)
                    .or(client.quota().await.ok());
                let _ = evt_tx.send(WorkerEvent::ServerMetadataLoaded {
                    server_info,
                    current_user,
                    quota,
                });
            }
            let _ = evt_tx.send(WorkerEvent::RemoteSharesLoaded(res));
            (waker)();
        }
        WorkerCommand::DeleteRemoteShare {
            share_id,
            server_url,
            api_token,
        } => {
            let client = HttpCloudClient::new(server_url, api_token);
            let res = match client.delete_clip(&share_id).await {
                Ok(()) => Ok(()),
                Err(_) => client
                    .delete_share(&share_id)
                    .await
                    .map_err(|e| e.to_string()),
            };
            if res.is_ok() {
                let quota = client.quota().await.ok();
                let current_user = client.current_user().await.ok();
                let server_info = client.server_info().await.ok();
                let _ = evt_tx.send(WorkerEvent::ServerMetadataLoaded {
                    server_info,
                    current_user,
                    quota,
                });
            }
            let _ = evt_tx.send(WorkerEvent::RemoteShareDeleted {
                share_id,
                result: res,
            });
            (waker)();
        }
        WorkerCommand::CopyToClipboard(text) => {
            let copied = copy_text_to_clipboard(&text).await;
            let msg = if copied {
                format!("Copied to clipboard: {text}")
            } else {
                format!("Share link: {text} (install wl-copy or xclip for clipboard copy)")
            };
            let _ = evt_tx.send(WorkerEvent::Banner(msg));
            (waker)();
        }
        WorkerCommand::OpenUrl(url) => {
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                let _ = evt_tx.send(WorkerEvent::Banner(
                    "Invalid URL scheme (expected http:// or https://)".to_string(),
                ));
                (waker)();
                return;
            }
            match Command::new("xdg-open").arg(&url).spawn() {
                Ok(_) => {
                    let _ = evt_tx.send(WorkerEvent::Banner(format!("Opened {url}")));
                }
                Err(err) => {
                    let _ = evt_tx.send(WorkerEvent::Banner(format!("Failed to open URL: {err}")));
                }
            }
            (waker)();
        }
        WorkerCommand::StartMicTest(source_id) => {
            svc.mic_test.start_test(source_id, None);
        }
        WorkerCommand::StartMicMonitor(source_id) => {
            svc.mic_test.start_monitor(source_id);
        }
        WorkerCommand::StopMicTest => {
            svc.mic_test.stop();
        }
        WorkerCommand::PlayMicTestSample => {
            svc.mic_test.play_sample(None);
        }
        WorkerCommand::ClearMicTest => {
            svc.mic_test.clear();
        }
        WorkerCommand::RestartReplayBuffer(cfg) => {
            let _ = svc.capture.stop().await;
            match svc.capture.start_replay_buffer(&cfg).await {
                Ok(()) => {
                    let st = svc.capture.status().await;
                    let _ = evt_tx.send(WorkerEvent::CaptureStatusChanged(st));
                    let _ = evt_tx.send(WorkerEvent::Banner(
                        "Replay buffer restarted with updated capture settings".to_string(),
                    ));
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
    }
}

async fn copy_text_to_clipboard(text: &str) -> bool {
    use tokio::io::AsyncWriteExt;
    let candidates: [(&str, &[&str]); 3] = [
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
    ];
    for (bin, args) in candidates {
        let Ok(mut child) = Command::new(bin)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        else {
            continue;
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }
        if let Ok(status) = child.wait().await
            && status.success()
        {
            return true;
        }
    }
    false
}

async fn decode_thumbnail_rgba(
    path: &std::path::Path,
    width: u32,
    height: u32,
) -> std::result::Result<Vec<u8>, String> {
    let filter = format!(
        "scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color=0x161820ff"
    );
    let output = Command::new("ffmpeg")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(path)
        .arg("-vf")
        .arg(filter)
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgba")
        .arg("pipe:1")
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    let expected = (width as usize) * (height as usize) * 4;
    if output.stdout.len() != expected {
        return Err(format!(
            "unexpected thumbnail byte count: got {}, expected {expected}",
            output.stdout.len()
        ));
    }
    Ok(output.stdout)
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
    index_clip_with_origin(store, media, path, auto_generate_thumbnails, None, false).await
}

async fn index_clip_with_origin(
    store: &SqliteClipStore,
    media: &FfmpegMediaProcessor,
    path: &std::path::Path,
    auto_generate_thumbnails: bool,
    origin: Option<ClipOrigin>,
    overwrite_existing: bool,
) -> bool {
    let existing = store.get_by_file_path(path).await.ok().flatten();
    if existing.is_some() && !overwrite_existing {
        return false;
    }
    let Ok(probe) = media.probe(path).await else {
        return false;
    };
    let id = existing.as_ref().map(|c| c.id).unwrap_or_else(ClipId::new);
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
    if let Some(origin) = origin {
        store.upsert_clip_with_origin(&clip, origin).await.is_ok()
    } else {
        store.upsert_clip(&clip).await.is_ok()
    }
}

fn close_player_if_matches(svc: &WorkerServices, ids: &[ClipId]) {
    let active_matches = svc
        .active_player_clip
        .lock()
        .ok()
        .is_some_and(|g| g.as_ref().is_some_and(|(cid, _)| ids.contains(cid)));
    if active_matches {
        if let Ok(mut guard) = svc.active_player_clip.lock() {
            *guard = None;
        }
        svc.player.close();
    }
}

async fn emit_library_loaded(store: &SqliteClipStore, evt_tx: &Sender<WorkerEvent>) {
    let active_opts = ClipListOptions {
        exclude_trashed: true,
        ..Default::default()
    };
    let trash_opts = ClipListOptions {
        trashed_only: true,
        exclude_trashed: false,
        ..Default::default()
    };
    let filter = ClipFilter::default();
    if let Ok(clips) = store.list_clips_filtered(&filter, active_opts).await {
        let trashed_clips = store
            .list_clips_filtered(&filter, trash_opts)
            .await
            .unwrap_or_default();
        let origins = store.list_origins().await.unwrap_or_default();
        let mut favorites = HashSet::new();
        for c in clips.iter().chain(trashed_clips.iter()) {
            if store.is_favorite(c.id).await.unwrap_or(false) {
                favorites.insert(c.id);
            }
        }
        let mut trashed_at = HashMap::new();
        for c in &trashed_clips {
            if let Ok(Some(info)) = store.get_trash_info(c.id).await {
                trashed_at.insert(c.id, info.trashed_at);
            }
        }
        let _ = evt_tx.send(WorkerEvent::LibraryLoaded {
            clips,
            trashed_clips,
            favorites,
            origins,
            trashed_at,
        });
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

async fn execute_start_replay_buffer(
    svc: &Arc<WorkerServices>,
    cfg: &CaptureConfig,
    evt_tx: &Sender<WorkerEvent>,
    waker: &Waker,
) {
    match svc.capture.start_replay_buffer(cfg).await {
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

async fn execute_start_recording(
    svc: &Arc<WorkerServices>,
    cfg: &CaptureConfig,
    evt_tx: &Sender<WorkerEvent>,
    waker: &Waker,
) {
    match svc.capture.start_recording(cfg).await {
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

async fn execute_save_replay(
    svc: &Arc<WorkerServices>,
    evt_tx: &Sender<WorkerEvent>,
    waker: &Waker,
) {
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

async fn execute_stop_capture(
    svc: &Arc<WorkerServices>,
    evt_tx: &Sender<WorkerEvent>,
    waker: &Waker,
) {
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

async fn handle_hotkey_action(
    action: HotkeyAction,
    svc: &Arc<WorkerServices>,
    evt_tx: &Sender<WorkerEvent>,
    waker: &Waker,
) {
    match action {
        HotkeyAction::SaveReplay => {
            execute_save_replay(svc, evt_tx, waker).await;
        }
        HotkeyAction::ToggleReplayBuffer => {
            let status = svc.capture.status().await;
            if matches!(status, CaptureStatus::ReplayBuffering { .. }) {
                execute_stop_capture(svc, evt_tx, waker).await;
            } else {
                let cfg = svc
                    .config
                    .lock()
                    .map(|g| g.capture.clone())
                    .unwrap_or_default();
                execute_start_replay_buffer(svc, &cfg, evt_tx, waker).await;
            }
        }
        HotkeyAction::ToggleRecording => {
            let status = svc.capture.status().await;
            if matches!(status, CaptureStatus::Recording { .. }) {
                execute_stop_capture(svc, evt_tx, waker).await;
            } else {
                let cfg = svc
                    .config
                    .lock()
                    .map(|g| g.capture.clone())
                    .unwrap_or_default();
                execute_start_recording(svc, &cfg, evt_tx, waker).await;
            }
        }
    }
}

fn build_hotkey_bindings(
    hotkeys: &HotkeyBindings,
) -> std::result::Result<Vec<(HotkeyAction, HotkeySpec)>, String> {
    let mut out = Vec::with_capacity(3);
    for (action, raw) in [
        (HotkeyAction::SaveReplay, hotkeys.save_replay.as_str()),
        (
            HotkeyAction::ToggleReplayBuffer,
            hotkeys.toggle_replay.as_str(),
        ),
        (
            HotkeyAction::ToggleRecording,
            hotkeys.toggle_record.as_str(),
        ),
    ] {
        let spec = parse_hotkey(raw).map_err(|e| format!("invalid hotkey '{raw}': {e}"))?;
        out.push((action, spec));
    }
    Ok(out)
}

fn register_hotkeys(svc: &Arc<WorkerServices>, hotkeys: &HotkeyBindings) {
    if let Ok(mut guard) = svc.hotkey_manager.lock() {
        *guard = None;
    }
    match build_hotkey_bindings(hotkeys) {
        Ok(bindings) => {
            let mgr = HotkeyManager::start_auto(
                bindings,
                svc.hotkey_tx.clone(),
                svc.hotkey_status_tx.clone(),
            );
            if let Ok(mut guard) = svc.hotkey_manager.lock() {
                *guard = Some(mgr);
            }
        }
        Err(message) => {
            let _ = svc
                .hotkey_status_tx
                .send(GlobalHotkeyStatus::RegistrationError { message });
        }
    }
}
