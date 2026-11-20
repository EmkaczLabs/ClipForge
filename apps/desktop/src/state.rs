//! Application state for ClipForge Desktop wired to `clipforge-common` domain models.

use std::collections::HashSet;
use std::path::PathBuf;

use chrono::{Duration, Utc};
use clipforge_cloud_client::ShareVisibility;
use clipforge_common::{
    AppConfig, AudioTrackInfo, CaptureEvent, CaptureStatus, CaptureTarget, ClipId, ClipMetadata,
    CloudShareInfo, ContainerFormat, EncoderMode, GameInfo, QualityPreset, TrimRequest, VideoCodec,
};

use crate::config::load_app_config;
use crate::worker::{DependencyStatus, WorkerCommand, WorkerEvent, WorkerHandle};

/// Top-level navigation sections in the left sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NavSection {
    #[default]
    Dashboard,
    Library,
    Editor,
    Recording,
    Uploads,
    Settings,
}

impl NavSection {
    pub const ALL: [NavSection; 6] = [
        NavSection::Dashboard,
        NavSection::Library,
        NavSection::Editor,
        NavSection::Recording,
        NavSection::Uploads,
        NavSection::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NavSection::Dashboard => "Dashboard",
            NavSection::Library => "Clip Library",
            NavSection::Editor => "Video Editor",
            NavSection::Recording => "Recording",
            NavSection::Uploads => "Uploads",
            NavSection::Settings => "Settings",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            NavSection::Dashboard => "nav-dashboard",
            NavSection::Library => "nav-library",
            NavSection::Editor => "nav-editor",
            NavSection::Recording => "nav-recording",
            NavSection::Uploads => "nav-uploads",
            NavSection::Settings => "nav-settings",
        }
    }
}

/// Library display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LibraryViewMode {
    #[default]
    Grid,
    List,
}

/// Library filter preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LibraryFilter {
    #[default]
    All,
    Favorites,
    Shared,
}

/// Upload item state in the Uploads view.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum UploadStatus {
    Queued,
    Uploading { progress_pct: u8 },
    Completed,
    Failed(String),
}

/// Represents a clip upload or share entry.
#[derive(Debug, Clone)]
pub struct UploadEntry {
    pub clip_id: ClipId,
    pub title: String,
    pub game_name: String,
    pub status: UploadStatus,
    pub share: Option<CloudShareInfo>,
    pub is_public: bool,
}

/// Video Editor state for preview and non-destructive trim selection.
#[derive(Debug, Clone)]
pub struct EditorState {
    pub selected_clip_id: Option<ClipId>,
    pub playing: bool,
    pub playhead_ms: u64,
    pub trim_start_ms: u64,
    pub trim_end_ms: u64,
    pub reencode: bool,
    pub last_export_path: Option<PathBuf>,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            selected_clip_id: None,
            playing: false,
            playhead_ms: 0,
            trim_start_ms: 0,
            trim_end_ms: 15_000,
            reencode: false,
            last_export_path: None,
        }
    }
}

/// Hotkey bindings shown and configurable in the Recording view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyConfig {
    pub save_replay: String,
    pub toggle_replay: String,
    pub toggle_record: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            save_replay: "F9".to_string(),
            toggle_replay: "F10".to_string(),
            toggle_record: "F11".to_string(),
        }
    }
}

/// Complete UI state for ClipForge Desktop.
pub struct AppState {
    pub nav: NavSection,
    pub config: AppConfig,
    pub capture_status: CaptureStatus,
    pub deps: DependencyStatus,
    pub clips: Vec<ClipMetadata>,
    pub using_sample_clips: bool,
    pub favorite_ids: HashSet<ClipId>,
    pub library_mode: LibraryViewMode,
    pub library_filter: LibraryFilter,
    pub library_game_filter: Option<String>,
    pub editor: EditorState,
    pub uploads: Vec<UploadEntry>,
    pub hotkeys: HotkeyConfig,
    pub status_banner: Option<String>,
    pub worker: Option<WorkerHandle>,
}

impl Default for AppState {
    fn default() -> Self {
        let config = load_app_config();
        let clips = seed_initial_clips();
        let first_clip = clips.first().cloned();
        let mut favorite_ids = HashSet::new();
        if let Some(c) = clips.first() {
            favorite_ids.insert(c.id);
        }

        let editor = if let Some(c) = &first_clip {
            EditorState {
                selected_clip_id: Some(c.id),
                playing: false,
                playhead_ms: 0,
                trim_start_ms: 0,
                trim_end_ms: c.duration_ms,
                reencode: false,
                last_export_path: None,
            }
        } else {
            EditorState::default()
        };

        let uploads = clips
            .iter()
            .filter_map(|c| {
                c.cloud_share.as_ref().map(|share| UploadEntry {
                    clip_id: c.id,
                    title: c.title.clone(),
                    game_name: c
                        .game
                        .as_ref()
                        .map(|g| g.name.clone())
                        .unwrap_or_else(|| "Desktop".to_string()),
                    status: UploadStatus::Completed,
                    share: Some(share.clone()),
                    is_public: true,
                })
            })
            .collect();

        Self {
            nav: NavSection::Dashboard,
            config,
            capture_status: CaptureStatus::Idle,
            deps: DependencyStatus::default(),
            clips,
            using_sample_clips: true,
            favorite_ids,
            library_mode: LibraryViewMode::Grid,
            library_filter: LibraryFilter::All,
            library_game_filter: None,
            editor,
            uploads,
            hotkeys: HotkeyConfig::default(),
            status_banner: None,
            worker: None,
        }
    }
}

impl AppState {
    /// Returns the currently selected clip in the Video Editor, if any.
    pub fn selected_clip(&self) -> Option<&ClipMetadata> {
        let id = self.editor.selected_clip_id?;
        self.clips.iter().find(|c| c.id == id)
    }

    /// Selects a clip for editing and navigates to the Video Editor view.
    pub fn open_in_editor(&mut self, id: ClipId) {
        if let Some(clip) = self.clips.iter().find(|c| c.id == id) {
            self.editor.selected_clip_id = Some(id);
            self.editor.playing = false;
            self.editor.playhead_ms = 0;
            self.editor.trim_start_ms = 0;
            self.editor.trim_end_ms = clip.duration_ms;
            self.nav = NavSection::Editor;
        }
    }

    /// Toggles favorite status for a clip.
    pub fn toggle_favorite(&mut self, id: ClipId) {
        let fav = if self.favorite_ids.remove(&id) {
            false
        } else {
            self.favorite_ids.insert(id);
            true
        };
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::ToggleFavorite { id, favorite: fav });
        }
    }

    /// Returns filtered clips for the Clip Library view.
    pub fn filtered_clips(&self) -> Vec<&ClipMetadata> {
        self.clips
            .iter()
            .filter(|c| match self.library_filter {
                LibraryFilter::All => true,
                LibraryFilter::Favorites => self.favorite_ids.contains(&c.id),
                LibraryFilter::Shared => c.cloud_share.is_some(),
            })
            .filter(|c| match &self.library_game_filter {
                Some(game) => c.game.as_ref().is_some_and(|g| &g.name == game),
                None => true,
            })
            .collect()
    }

    /// Toggles replay buffering state via `GsrCaptureBackend` (or in-memory in headless tests).
    pub fn toggle_replay_buffer(&mut self) {
        if let Some(worker) = &self.worker {
            match self.capture_status {
                CaptureStatus::ReplayBuffering { .. } => worker.send(WorkerCommand::StopCapture),
                _ => worker.send(WorkerCommand::StartReplayBuffer(
                    self.config.capture.clone(),
                )),
            }
            return;
        }
        match self.capture_status {
            CaptureStatus::ReplayBuffering { .. } => {
                self.capture_status = CaptureStatus::Idle;
                self.status_banner = Some("Replay buffer stopped".to_string());
            }
            _ => {
                self.capture_status = CaptureStatus::ReplayBuffering {
                    started_at: Utc::now(),
                    buffer_secs: self.config.capture.replay_buffer_secs,
                };
                self.status_banner = Some(format!(
                    "Replay buffer active ({}s)",
                    self.config.capture.replay_buffer_secs
                ));
            }
        }
    }

    /// Toggles standard recording state via `GsrCaptureBackend` (or in-memory in headless tests).
    pub fn toggle_recording(&mut self) {
        if let Some(worker) = &self.worker {
            match self.capture_status {
                CaptureStatus::Recording { .. } => worker.send(WorkerCommand::StopCapture),
                _ => worker.send(WorkerCommand::StartRecording(self.config.capture.clone())),
            }
            return;
        }
        match self.capture_status {
            CaptureStatus::Recording { .. } => {
                self.capture_status = CaptureStatus::Idle;
                self.status_banner = Some("Recording stopped".to_string());
            }
            _ => {
                let output_path = self.config.clips_dir.join("recording-live.mp4");
                self.capture_status = CaptureStatus::Recording {
                    started_at: Utc::now(),
                    output_path,
                    paused: false,
                };
                self.status_banner = Some("Recording started".to_string());
            }
        }
    }

    /// Triggers a replay save via `GsrCaptureBackend` (or creates a local entry in headless tests).
    pub fn trigger_save_replay(&mut self) {
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::SaveReplay);
            return;
        }
        let now = Utc::now();
        let id = ClipId::new();
        let file_name = format!("replay-{}.mp4", now.format("%Y%m%d-%H%M%S"));
        let new_clip = ClipMetadata {
            id,
            title: format!("Replay {}", now.format("%H:%M:%S")),
            game: Some(GameInfo {
                id: "desktop".to_string(),
                name: "Desktop Capture".to_string(),
                executable: None,
                process_name: None,
            }),
            file_path: self.config.clips_dir.join(file_name),
            thumbnail_path: None,
            created_at: now,
            duration_ms: u64::from(self.config.capture.replay_buffer_secs) * 1000,
            width: 1920,
            height: 1080,
            fps: self.config.capture.fps as f32,
            size_bytes: 28_400_000,
            codec: codec_label(self.config.capture.video_codec).to_string(),
            audio_tracks: vec![AudioTrackInfo {
                index: 0,
                codec: "opus".to_string(),
                channels: 2,
                sample_rate: 48_000,
                label: Some("Desktop Audio".to_string()),
            }],
            tags: vec!["replay".to_string()],
            cloud_share: None,
            notes: None,
        };
        self.clips.insert(0, new_clip);
        self.status_banner = Some(format!(
            "Saved replay ({}s buffer)",
            self.config.capture.replay_buffer_secs
        ));
    }

    /// Queues a trim export request via the background worker (or validates locally in tests).
    pub fn export_trimmed_clip(&mut self) {
        let Some(clip) = self.selected_clip().cloned() else {
            self.status_banner = Some("No clip selected for trim export".to_string());
            return;
        };
        let is_sample = is_sample_clip(&clip);
        let out_name = format!("{}-trimmed.mp4", clip.id);
        let output_path = self.config.clips_dir.join(out_name);
        let req = TrimRequest {
            source_path: clip.file_path,
            output_path,
            start_ms: self.editor.trim_start_ms,
            end_ms: self.editor.trim_end_ms,
            reencode: self.editor.reencode,
            video_codec: Some(self.config.capture.video_codec),
        };

        if let Some(worker) = &self.worker {
            if is_sample || !req.source_path.is_file() {
                self.status_banner = Some(
                    "Sample clip cannot be exported — record or scan a real clip first".to_string(),
                );
                return;
            }
            if let Err(err) = req.validate() {
                self.status_banner = Some(format!("Invalid trim range: {err}"));
                return;
            }
            self.status_banner = Some("Exporting trimmed clip...".to_string());
            worker.send(WorkerCommand::TrimClip(req));
        } else {
            match req.validate() {
                Ok(()) => {
                    self.editor.last_export_path = Some(req.output_path.clone());
                    self.status_banner =
                        Some(format!("Trim validated: {}", req.output_path.display()));
                }
                Err(err) => {
                    self.status_banner = Some(format!("Trim error: {err}"));
                }
            }
        }
    }

    /// Queues a clip upload entry in the Uploads view and dispatches `HttpCloudClient` upload.
    pub fn queue_upload(&mut self, id: ClipId) {
        let Some(clip) = self.clips.iter().find(|c| c.id == id).cloned() else {
            return;
        };
        if self.uploads.iter().any(|u| u.clip_id == id) {
            self.nav = NavSection::Uploads;
            return;
        }
        self.uploads.insert(
            0,
            UploadEntry {
                clip_id: id,
                title: clip.title.clone(),
                game_name: clip
                    .game
                    .as_ref()
                    .map(|g| g.name.clone())
                    .unwrap_or_else(|| "Desktop".to_string()),
                status: UploadStatus::Queued,
                share: None,
                is_public: true,
            },
        );
        if let Some(worker) = &self.worker {
            if is_sample_clip(&clip) || !clip.file_path.is_file() {
                self.status_banner = Some(
                    "Sample clip cannot be uploaded — record or scan a real clip first".to_string(),
                );
                return;
            }
            worker.send(WorkerCommand::UploadClip {
                clip: Box::new(clip),
                visibility: ShareVisibility::Public,
                server_url: self.config.server_url.clone(),
                api_token: self.config.api_token.clone(),
            });
        }
        self.status_banner = Some("Clip queued for upload".to_string());
        self.nav = NavSection::Uploads;
    }

    /// Toggles public/private visibility for an uploaded share.
    pub fn toggle_share_visibility(&mut self, clip_id: ClipId) {
        let mut next_vis = None;
        let mut share_id = None;
        if let Some(u) = self.uploads.iter_mut().find(|u| u.clip_id == clip_id) {
            u.is_public = !u.is_public;
            next_vis = Some(if u.is_public {
                ShareVisibility::Public
            } else {
                ShareVisibility::Private
            });
            share_id = u.share.as_ref().map(|s| s.share_id.clone());
        }
        if let (Some(worker), Some(sid), Some(vis)) = (&self.worker, share_id, next_vis) {
            worker.send(WorkerCommand::SetShareVisibility {
                share_id: sid,
                visibility: vis,
                server_url: self.config.server_url.clone(),
                api_token: self.config.api_token.clone(),
            });
        }
    }

    /// Saves the current `AppConfig` asynchronously via the background worker.
    pub fn save_config(&mut self) {
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::SaveConfig(self.config.clone()));
        } else {
            self.status_banner = Some("Config updated in memory".to_string());
        }
    }

    /// Drains any pending events from the background worker.
    pub fn poll_worker(&mut self) -> bool {
        let mut changed = false;
        let events: Vec<WorkerEvent> = if let Some(worker) = &self.worker {
            let mut buf = Vec::new();
            while let Some(ev) = worker.try_recv() {
                buf.push(ev);
            }
            buf
        } else {
            Vec::new()
        };

        for ev in events {
            changed = true;
            match ev {
                WorkerEvent::DependenciesProbed(deps) => {
                    self.deps = deps;
                }
                WorkerEvent::LibraryLoaded { clips, favorites } => {
                    if !clips.is_empty() {
                        self.using_sample_clips = false;
                        self.clips = clips;
                        self.favorite_ids = favorites;
                        self.uploads.retain(|u| {
                            u.share
                                .as_ref()
                                .is_none_or(|sh| sh.share_id != "shr_mirage_01")
                        });
                        let selected_valid = self
                            .editor
                            .selected_clip_id
                            .is_some_and(|id| self.clips.iter().any(|c| c.id == id));
                        if !selected_valid && let Some(first) = self.clips.first() {
                            self.editor.selected_clip_id = Some(first.id);
                            self.editor.trim_start_ms = 0;
                            self.editor.trim_end_ms = first.duration_ms;
                        }
                    }
                }
                WorkerEvent::CaptureStatusChanged(status) => {
                    self.capture_status = status;
                }
                WorkerEvent::CaptureEventReceived(cap_ev) => match cap_ev {
                    CaptureEvent::StatusChanged(status) => {
                        self.capture_status = status;
                    }
                    CaptureEvent::ReplaySaved { path, .. }
                    | CaptureEvent::RecordingFinished { path, .. } => {
                        self.status_banner = Some(format!("Captured: {}", path.display()));
                        if let Some(worker) = &self.worker {
                            worker.send(WorkerCommand::ScanLibrary(self.config.clone()));
                        }
                    }
                    CaptureEvent::Warning(msg) | CaptureEvent::Error(msg) => {
                        self.status_banner = Some(msg);
                    }
                },
                WorkerEvent::ConfigSaved(Ok(path)) => {
                    self.status_banner = Some(format!("Saved config to {}", path.display()));
                }
                WorkerEvent::ConfigSaved(Err(err)) => {
                    self.status_banner = Some(format!("Failed to save config: {err}"));
                }
                WorkerEvent::TrimCompleted(Ok(path)) => {
                    self.editor.last_export_path = Some(path.clone());
                    self.status_banner = Some(format!("Trimmed clip exported: {}", path.display()));
                    if let Some(worker) = &self.worker {
                        worker.send(WorkerCommand::ScanLibrary(self.config.clone()));
                    }
                }
                WorkerEvent::TrimCompleted(Err(err)) => {
                    self.status_banner = Some(format!("Trim failed: {err}"));
                }
                WorkerEvent::UploadCompleted { clip_id, result } => {
                    if let Some(entry) = self.uploads.iter_mut().find(|u| u.clip_id == clip_id) {
                        match result {
                            Ok(summary) => {
                                entry.status = UploadStatus::Completed;
                                entry.is_public = summary.visibility == ShareVisibility::Public;
                                entry.share = Some(summary.into());
                                self.status_banner = Some("Clip uploaded successfully".to_string());
                            }
                            Err(err) => {
                                entry.status = UploadStatus::Failed(err.clone());
                                self.status_banner = Some(format!("Upload failed: {err}"));
                            }
                        }
                    }
                }
                WorkerEvent::VisibilityUpdated { share_id, result } => {
                    if let Err(err) = result {
                        self.status_banner =
                            Some(format!("Visibility update failed for {share_id}: {err}"));
                    }
                }
                WorkerEvent::Banner(msg) => {
                    self.status_banner = Some(msg);
                }
            }
        }
        changed
    }

    /// Cycles capture target in the Recording view.
    pub fn set_capture_target(&mut self, target: CaptureTarget) {
        self.config.capture.capture_target = target;
    }

    /// Sets encoder mode (`Gpu` / `Cpu`).
    pub fn set_encoder(&mut self, encoder: EncoderMode) {
        self.config.capture.encoder = encoder;
    }

    /// Sets quality preset.
    pub fn set_quality(&mut self, quality: QualityPreset) {
        self.config.capture.quality = quality;
    }

    /// Sets container format.
    pub fn set_container(&mut self, container: ContainerFormat) {
        self.config.capture.container = container;
    }

    /// Launches the clip in the system video player via the background worker.
    pub fn open_clip_in_player(&mut self, id: ClipId) {
        let Some(clip) = self.clips.iter().find(|c| c.id == id).cloned() else {
            return;
        };
        if is_sample_clip(&clip) || !clip.file_path.is_file() {
            self.status_banner = Some(
                "Sample clip has no video file on disk — record a real clip to play it".to_string(),
            );
            return;
        }
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::OpenInPlayer(clip.file_path));
        } else {
            self.status_banner = Some(format!("Play clip: {}", clip.file_path.display()));
        }
    }

    /// Resets `CaptureStatus::Error` back to `CaptureStatus::Idle` for recovery.
    pub fn clear_capture_error(&mut self) {
        if matches!(self.capture_status, CaptureStatus::Error { .. }) {
            self.capture_status = CaptureStatus::Idle;
            self.status_banner = None;
        }
    }
}

/// Returns `true` when `clip` is an in-memory sample/demo entry rather than a real recording.
pub fn is_sample_clip(clip: &ClipMetadata) -> bool {
    clip.tags.iter().any(|t| t == "sample")
}

fn codec_label(codec: VideoCodec) -> &'static str {
    match codec {
        VideoCodec::Auto => "h264",
        VideoCodec::H264 => "h264",
        VideoCodec::Hevc => "hevc",
        VideoCodec::Av1 => "av1",
        VideoCodec::Vp8 => "vp8",
        VideoCodec::Vp9 => "vp9",
    }
}

/// Formats milliseconds as `MM:SS`.
pub fn format_duration_ms(ms: u64) -> String {
    let total_secs = ms / 1000;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{mins:02}:{secs:02}")
}

/// Formats bytes as human-readable MB/GB.
pub fn format_bytes(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * MB;
    if bytes as f64 >= GB {
        format!("{:.2} GB", bytes as f64 / GB)
    } else {
        format!("{:.1} MB", bytes as f64 / MB)
    }
}

fn seed_initial_clips() -> Vec<ClipMetadata> {
    let now = Utc::now();
    vec![
        ClipMetadata {
            id: ClipId::new(),
            title: "Ace Clutch on Mirage".to_string(),
            game: Some(GameInfo {
                id: "cs2".to_string(),
                name: "Counter-Strike 2".to_string(),
                executable: Some("cs2".to_string()),
                process_name: Some("cs2".to_string()),
            }),
            file_path: PathBuf::from("clips/cs2-ace-mirage.mp4"),
            thumbnail_path: None,
            created_at: now - Duration::minutes(18),
            duration_ms: 42_000,
            width: 1920,
            height: 1080,
            fps: 60.0,
            size_bytes: 46_200_000,
            codec: "h264".to_string(),
            audio_tracks: vec![AudioTrackInfo {
                index: 0,
                codec: "opus".to_string(),
                channels: 2,
                sample_rate: 48_000,
                label: Some("Game + Mic".to_string()),
            }],
            tags: vec![
                "sample".to_string(),
                "clutch".to_string(),
                "ace".to_string(),
            ],
            cloud_share: Some(CloudShareInfo {
                share_id: "shr_mirage_01".to_string(),
                url: "https://clips.local/s/shr_mirage_01".to_string(),
                uploaded_at: now - Duration::minutes(10),
                expires_at: None,
            }),
            notes: None,
        },
        ClipMetadata {
            id: ClipId::new(),
            title: "Aerial Double Tap".to_string(),
            game: Some(GameInfo {
                id: "rl".to_string(),
                name: "Rocket League".to_string(),
                executable: Some("RocketLeague".to_string()),
                process_name: None,
            }),
            file_path: PathBuf::from("clips/rl-double-tap.mp4"),
            thumbnail_path: None,
            created_at: now - Duration::hours(3),
            duration_ms: 24_000,
            width: 2560,
            height: 1440,
            fps: 120.0,
            size_bytes: 38_900_000,
            codec: "hevc".to_string(),
            audio_tracks: vec![],
            tags: vec!["sample".to_string(), "aerial".to_string()],
            cloud_share: None,
            notes: None,
        },
        ClipMetadata {
            id: ClipId::new(),
            title: "Boss Phase 2 No-Hit".to_string(),
            game: Some(GameInfo {
                id: "eldenring".to_string(),
                name: "Elden Ring".to_string(),
                executable: Some("eldenring.exe".to_string()),
                process_name: None,
            }),
            file_path: PathBuf::from("clips/er-no-hit.mp4"),
            thumbnail_path: None,
            created_at: now - Duration::days(1),
            duration_ms: 58_000,
            width: 1920,
            height: 1080,
            fps: 60.0,
            size_bytes: 61_500_000,
            codec: "h264".to_string(),
            audio_tracks: vec![],
            tags: vec!["sample".to_string(), "boss".to_string()],
            cloud_share: None,
            notes: None,
        },
    ]
}
