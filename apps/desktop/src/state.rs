//! Application state for ClipForge Desktop wired to `clipforge-common` domain models.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use clipforge_capture::{
    AudioDeviceInfo, AudioRoutingPlan, AudioTrackTarget, GlobalHotkeyStatus, HotkeyAction,
    MonitorInfo, format_routed_audio_source, is_mic_source, parse_routed_audio_source,
    resolve_audio_routing,
};
use clipforge_cloud_client::{
    QuotaDto, RemoteClipSummary, ServerInfoDto, ShareVisibility, UserInfo,
};
use clipforge_common::{
    AppConfig, AudioCodec, AudioTrackInfo, CaptureEvent, CaptureStatus, CaptureTarget, ClipId,
    ClipMetadata, CloudShareInfo, ContainerFormat, EncoderMode, GameInfo, HotkeyBindings,
    QualityPreset, TrimRequest, VideoCodec,
};
use clipforge_library::{ClipOrigin, ClipSort};
use clipforge_media::{
    DBFS_FLOOR, MicDiagnosticSummary, MicFeedbackKind, MicTestEvent, MicTestPhase,
};

use crate::config::{delete_api_token, load_api_token, load_app_config, store_api_token};
use crate::worker::{DependencyStatus, WorkerCommand, WorkerEvent, WorkerHandle};

/// Top-level navigation sections in the left sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NavSection {
    #[allow(dead_code)]
    Dashboard,
    #[default]
    Library,
    Editor,
    Recording,
    Uploads,
    Settings,
}

impl NavSection {
    pub const ALL: [NavSection; 5] = [
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
            NavSection::Uploads => "Shared & Cloud",
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

/// Settings category tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum SettingsTab {
    #[default]
    General,
    Storage,
    Recording,
    Audio,
    Shortcuts,
    Appearance,
    Sharing,
    About,
}

#[allow(dead_code)]
impl SettingsTab {
    pub const ALL: [SettingsTab; 8] = [
        SettingsTab::General,
        SettingsTab::Storage,
        SettingsTab::Recording,
        SettingsTab::Audio,
        SettingsTab::Shortcuts,
        SettingsTab::Appearance,
        SettingsTab::Sharing,
        SettingsTab::About,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SettingsTab::General => "General",
            SettingsTab::Storage => "Storage & Library",
            SettingsTab::Recording => "Recording Defaults",
            SettingsTab::Audio => "Audio",
            SettingsTab::Shortcuts => "Keyboard Shortcuts",
            SettingsTab::Appearance => "Appearance",
            SettingsTab::Sharing => "Sharing & Server",
            SettingsTab::About => "About",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            SettingsTab::General => "settings-tab-general",
            SettingsTab::Storage => "settings-tab-storage",
            SettingsTab::Recording => "settings-tab-recording",
            SettingsTab::Audio => "settings-tab-audio",
            SettingsTab::Shortcuts => "settings-tab-shortcuts",
            SettingsTab::Appearance => "settings-tab-appearance",
            SettingsTab::Sharing => "settings-tab-sharing",
            SettingsTab::About => "settings-tab-about",
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
    Recorded,
    Trimmed,
    Favorites,
    Shared,
    Trash,
}

/// Pending destructive/trash confirmation action in the Clip Library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    MoveToTrash(Vec<ClipId>),
    RestoreFromTrash(Vec<ClipId>),
    PermanentlyDelete(Vec<ClipId>),
}

/// Floating context menu state for a clip card or row.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryContextMenu {
    pub clip_id: ClipId,
    pub x: f32,
    pub y: f32,
    pub is_trashed: bool,
}

/// Upload item state in the Uploads view.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum UploadStatus {
    Queued,
    Uploading { progress_pct: u8 },
    Completed,
    Failed(String),
    Cancelled,
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
    pub video_pixels: rune::Pixels,
    pub has_video_frame: bool,
    pub video_width: u32,
    pub video_height: u32,
    pub decoder_error: Option<String>,
    pub timeline_thumbs: Vec<rune::Pixels>,
    pub volume: f32,
    pub muted: bool,
    pub has_audio: bool,
    pub audio_error: Option<String>,
    #[allow(dead_code)]
    pub audio_track: Option<usize>,
    pub timeline_zoom: f32,
    pub exporting: bool,
    pub export_progress: Option<f32>,
    pub export_error: Option<String>,
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
            video_pixels: rune::Pixels::new(),
            has_video_frame: false,
            video_width: 0,
            video_height: 0,
            decoder_error: None,
            timeline_thumbs: Vec::new(),
            volume: 1.0,
            muted: false,
            has_audio: false,
            audio_error: None,
            audio_track: None,
            timeline_zoom: 1.0,
            exporting: false,
            export_progress: None,
            export_error: None,
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

/// Live state for the standalone PipeWire Microphone Test service.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MicTestState {
    pub phase: MicTestPhase,
    pub selected_source: Option<String>,
    pub rms_dbfs: f32,
    pub peak_dbfs: f32,
    pub elapsed_ms: u64,
    pub feedback: Option<MicFeedbackKind>,
    pub diagnostic_summary: Option<MicDiagnosticSummary>,
    pub error: Option<String>,
}

impl Default for MicTestState {
    fn default() -> Self {
        Self {
            phase: MicTestPhase::Idle,
            selected_source: None,
            rms_dbfs: DBFS_FLOOR,
            peak_dbfs: DBFS_FLOOR,
            elapsed_ms: 0,
            feedback: None,
            diagnostic_summary: None,
            error: None,
        }
    }
}

/// Snapshot of audio track routing and encoded stream diagnostics for Recording Studio.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct AudioDiagnosticsSnapshot {
    pub routing_plan: AudioRoutingPlan,
    pub last_saved_clip_title: Option<String>,
    pub last_saved_stream_count: usize,
    pub last_saved_codec: Option<String>,
    pub last_saved_channels: Option<u32>,
    pub mic_signal_detected: Option<bool>,
}

/// Complete UI state for ClipForge Desktop.
pub struct AppState {
    pub nav: NavSection,
    pub config: AppConfig,
    pub capture_status: CaptureStatus,
    pub deps: DependencyStatus,
    pub clips: Vec<ClipMetadata>,
    pub trashed_clips: Vec<ClipMetadata>,
    pub using_sample_clips: bool,
    pub favorite_ids: HashSet<ClipId>,
    pub clip_origins: HashMap<ClipId, ClipOrigin>,
    pub trashed_at: HashMap<ClipId, DateTime<Utc>>,
    pub library_mode: LibraryViewMode,
    pub library_filter: LibraryFilter,
    pub library_sort: ClipSort,
    pub library_search: rune::TextEdit,
    pub library_game_filter: Option<String>,
    #[allow(dead_code)]
    pub inspector_open: bool,
    pub library_thumbs: HashMap<ClipId, rune::Pixels>,
    pub selected_clip_ids: HashSet<ClipId>,
    pub selection_anchor: Option<ClipId>,
    pub pending_confirm: Option<ConfirmAction>,
    pub renaming_clip_id: Option<ClipId>,
    pub rename_edit: rune::TextEdit,
    pub context_menu: Option<LibraryContextMenu>,
    pub editor: EditorState,
    #[allow(dead_code)]
    pub is_wayland: bool,
    pub available_monitors: Vec<MonitorInfo>,
    pub available_audio_devices: Vec<AudioDeviceInfo>,
    pub available_app_audio: Vec<AudioDeviceInfo>,
    #[allow(dead_code)]
    pub settings_tab: SettingsTab,
    #[allow(dead_code)]
    pub settings_search: rune::TextEdit,
    #[allow(dead_code)]
    pub clips_dir_edit: rune::TextEdit,
    pub uploads: Vec<UploadEntry>,
    pub server_connected: bool,
    pub server_health_ok: Option<bool>,
    pub server_error: Option<String>,
    pub server_info: Option<ServerInfoDto>,
    pub current_user: Option<UserInfo>,
    pub quota: Option<QuotaDto>,
    pub remote_shares: Vec<RemoteClipSummary>,
    #[allow(dead_code)]
    pub selected_upload_id: Option<ClipId>,
    pub server_url_edit: rune::TextEdit,
    pub api_token_edit: rune::TextEdit,
    #[allow(dead_code)]
    pub server_user_edit: rune::TextEdit,
    #[allow(dead_code)]
    pub server_pass_edit: rune::TextEdit,
    pub confirm_remote_delete: Option<ClipId>,
    pub hotkeys: HotkeyConfig,
    pub hotkey_status: GlobalHotkeyStatus,
    #[allow(dead_code)]
    pub audio_config_dirty_while_active: bool,
    #[allow(dead_code)]
    pub mic_test: MicTestState,
    pub status_banner: Option<String>,
    pub worker: Option<WorkerHandle>,
}

impl Default for AppState {
    fn default() -> Self {
        let mut config = load_app_config();
        if config.api_token.is_none()
            && let Some(url) = config.server_url.as_deref()
        {
            config.api_token = load_api_token(url);
        }
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
                ..EditorState::default()
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

        let hotkeys = HotkeyConfig {
            save_replay: config.hotkeys.save_replay.clone(),
            toggle_replay: config.hotkeys.toggle_replay.clone(),
            toggle_record: config.hotkeys.toggle_record.clone(),
        };
        let is_wayland = std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false);
        let server_connected = config
            .server_url
            .as_ref()
            .is_some_and(|u| !u.trim().is_empty());
        let clips_dir_edit = rune::TextEdit::new(config.clips_dir.display().to_string());
        let server_url_edit = rune::TextEdit::new(config.server_url.clone().unwrap_or_default());
        let api_token_edit = rune::TextEdit::new(config.api_token.clone().unwrap_or_default());

        Self {
            nav: NavSection::Library,
            config,
            capture_status: CaptureStatus::Idle,
            deps: DependencyStatus::default(),
            clips,
            trashed_clips: Vec::new(),
            using_sample_clips: true,
            favorite_ids,
            clip_origins: HashMap::new(),
            trashed_at: HashMap::new(),
            library_mode: LibraryViewMode::Grid,
            library_filter: LibraryFilter::All,
            library_sort: ClipSort::CreatedDesc,
            library_search: rune::TextEdit::default(),
            library_game_filter: None,
            inspector_open: true,
            library_thumbs: HashMap::new(),
            selected_clip_ids: first_clip.iter().map(|c| c.id).collect(),
            selection_anchor: first_clip.map(|c| c.id),
            pending_confirm: None,
            renaming_clip_id: None,
            rename_edit: rune::TextEdit::default(),
            context_menu: None,
            editor,
            is_wayland,
            available_monitors: Vec::new(),
            available_audio_devices: Vec::new(),
            available_app_audio: Vec::new(),
            settings_tab: SettingsTab::General,
            settings_search: rune::TextEdit::default(),
            clips_dir_edit,
            uploads,
            server_connected,
            server_health_ok: None,
            server_error: None,
            server_info: None,
            current_user: None,
            quota: None,
            remote_shares: Vec::new(),
            selected_upload_id: None,
            server_url_edit,
            api_token_edit,
            server_user_edit: rune::TextEdit::default(),
            server_pass_edit: rune::TextEdit::default(),
            confirm_remote_delete: None,
            hotkeys,
            hotkey_status: GlobalHotkeyStatus::FocusedWindowOnly { reason: None },
            audio_config_dirty_while_active: false,
            mic_test: MicTestState::default(),
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

    /// Finds a clip by ID across active and trashed clips.
    pub fn find_clip(&self, id: ClipId) -> Option<&ClipMetadata> {
        self.clips
            .iter()
            .chain(self.trashed_clips.iter())
            .find(|c| c.id == id)
    }

    /// Returns the currently selected clip for the Library detail panel (including Trash view).
    pub fn selected_library_clip(&self) -> Option<&ClipMetadata> {
        let id = self.editor.selected_clip_id?;
        if self.library_filter == LibraryFilter::Trash {
            self.trashed_clips.iter().find(|c| c.id == id)
        } else {
            self.clips.iter().find(|c| c.id == id)
        }
    }

    /// Returns the origin of a clip (using SQLite origin map with metadata fallback).
    pub fn clip_origin(&self, clip: &ClipMetadata) -> ClipOrigin {
        if let Some(origin) = self.clip_origins.get(&clip.id) {
            return *origin;
        }
        if is_sample_clip(clip) {
            ClipOrigin::Sample
        } else if clip.tags.iter().any(|t| t == "trimmed")
            || clip
                .file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.ends_with("-trim"))
        {
            ClipOrigin::Trimmed
        } else {
            ClipOrigin::Recorded
        }
    }

    /// Handles a clip card/row click with optional Ctrl/Shift modifiers for multi-selection.
    pub fn select_clip_with_mods(&mut self, id: ClipId, mods: rune::Mods) {
        self.context_menu = None;
        if mods.shift {
            let order: Vec<ClipId> = self.filtered_clips().into_iter().map(|c| c.id).collect();
            let anchor = self
                .selection_anchor
                .or(self.editor.selected_clip_id)
                .unwrap_or(id);
            if let (Some(a_idx), Some(b_idx)) = (
                order.iter().position(|cid| *cid == anchor),
                order.iter().position(|cid| *cid == id),
            ) {
                if !(mods.ctrl || mods.cmd) {
                    self.selected_clip_ids.clear();
                }
                let (lo, hi) = if a_idx <= b_idx {
                    (a_idx, b_idx)
                } else {
                    (b_idx, a_idx)
                };
                for cid in &order[lo..=hi] {
                    self.selected_clip_ids.insert(*cid);
                }
            } else {
                self.selected_clip_ids.insert(id);
                self.selection_anchor = Some(id);
            }
            if self.library_filter == LibraryFilter::Trash {
                self.editor.selected_clip_id = Some(id);
            } else {
                self.select_clip(id);
            }
            return;
        }

        if mods.ctrl || mods.cmd {
            self.selection_anchor = Some(id);
            if self.selected_clip_ids.contains(&id) {
                self.selected_clip_ids.remove(&id);
                if self.editor.selected_clip_id == Some(id) {
                    let next = self.selected_clip_ids.iter().copied().next();
                    if let Some(next_id) = next {
                        if self.library_filter == LibraryFilter::Trash {
                            self.editor.selected_clip_id = Some(next_id);
                        } else {
                            self.select_clip(next_id);
                        }
                    } else {
                        self.editor.selected_clip_id = None;
                    }
                }
            } else {
                self.selected_clip_ids.insert(id);
                if self.library_filter == LibraryFilter::Trash {
                    self.editor.selected_clip_id = Some(id);
                } else {
                    self.select_clip(id);
                }
            }
            return;
        }

        self.selected_clip_ids.clear();
        self.selected_clip_ids.insert(id);
        self.selection_anchor = Some(id);
        if self.library_filter == LibraryFilter::Trash {
            self.editor.selected_clip_id = Some(id);
        } else {
            self.select_clip(id);
        }
    }

    /// Toggles a clip's membership in the multi-selection set (explicit checkbox fallback).
    pub fn toggle_clip_in_selection(&mut self, id: ClipId) {
        self.selection_anchor = Some(id);
        if self.selected_clip_ids.contains(&id) {
            self.selected_clip_ids.remove(&id);
        } else {
            self.selected_clip_ids.insert(id);
        }
    }

    /// Selects all clips matching the active library filter.
    pub fn select_all_filtered(&mut self) {
        let ids: Vec<ClipId> = self.filtered_clips().into_iter().map(|c| c.id).collect();
        for id in ids {
            self.selected_clip_ids.insert(id);
        }
    }

    /// Clears the multi-selection set.
    pub fn clear_selection(&mut self) {
        self.selected_clip_ids.clear();
        self.selection_anchor = None;
    }

    /// Selects a clip (synchronized across Library and Editor) without switching views.
    pub fn select_clip(&mut self, id: ClipId) {
        let Some(clip) = self.clips.iter().find(|c| c.id == id).cloned() else {
            return;
        };
        if self.editor.selected_clip_id == Some(id) {
            return;
        }
        self.editor.selected_clip_id = Some(id);
        self.editor.playing = false;
        self.editor.playhead_ms = 0;
        self.editor.trim_start_ms = 0;
        self.editor.trim_end_ms = clip.duration_ms;
        self.editor.has_video_frame = false;
        self.editor.video_width = clip.width;
        self.editor.video_height = clip.height;
        self.editor.decoder_error = None;
        self.editor.timeline_thumbs.clear();
        self.editor.has_audio = !clip.audio_tracks.is_empty();
        self.editor.audio_error = None;
        self.editor.export_error = None;

        if let Some(worker) = &self.worker {
            if is_sample_clip(&clip) || !clip.file_path.is_file() {
                worker.send(WorkerCommand::ClosePlayer);
            } else if self.nav == NavSection::Editor {
                worker.send(WorkerCommand::OpenPlayer {
                    clip_id: id,
                    path: clip.file_path,
                    trim_range: Some((0, clip.duration_ms)),
                    pixels: self.editor.video_pixels.clone(),
                    autoplay: false,
                    volume: self.editor.volume,
                    muted: self.editor.muted,
                });
            } else {
                worker.send(WorkerCommand::ClosePlayer);
            }
        }
    }

    /// Selects a clip for editing, opens the decoder paused if backed by a file, and navigates to the Video Editor view.
    pub fn open_in_editor(&mut self, id: ClipId) {
        self.load_in_editor(id, false);
    }

    /// Selects a clip, navigates to the Video Editor view, and starts playback immediately.
    pub fn play_in_editor(&mut self, id: ClipId) {
        self.load_in_editor(id, true);
    }

    fn load_in_editor(&mut self, id: ClipId, autoplay: bool) {
        let Some(clip) = self.clips.iter().find(|c| c.id == id).cloned() else {
            return;
        };
        let changed = self.editor.selected_clip_id != Some(id);
        if changed {
            self.editor.selected_clip_id = Some(id);
            self.editor.playing = false;
            self.editor.playhead_ms = 0;
            self.editor.trim_start_ms = 0;
            self.editor.trim_end_ms = clip.duration_ms;
            self.editor.has_video_frame = false;
            self.editor.video_width = clip.width;
            self.editor.video_height = clip.height;
            self.editor.decoder_error = None;
            self.editor.timeline_thumbs.clear();
            self.editor.has_audio = !clip.audio_tracks.is_empty();
            self.editor.audio_error = None;
            self.editor.export_error = None;
        }
        self.nav = NavSection::Editor;

        if let Some(worker) = &self.worker {
            if is_sample_clip(&clip) || !clip.file_path.is_file() {
                worker.send(WorkerCommand::ClosePlayer);
                if autoplay {
                    self.editor.playing = false;
                    self.editor.decoder_error = Some(
                        "Sample clip has no video file on disk — record or scan a real clip to play it"
                            .to_string(),
                    );
                }
            } else {
                if autoplay {
                    self.editor.playing = true;
                    self.editor.decoder_error = None;
                }
                worker.send(WorkerCommand::OpenPlayer {
                    clip_id: id,
                    path: clip.file_path,
                    trim_range: Some((self.editor.trim_start_ms, self.editor.trim_end_ms)),
                    pixels: self.editor.video_pixels.clone(),
                    autoplay,
                    volume: self.editor.volume,
                    muted: self.editor.muted,
                });
            }
        } else if autoplay {
            if is_sample_clip(&clip) {
                self.editor.playing = false;
                self.editor.decoder_error = Some(
                    "Sample clip has no video file on disk — record or scan a real clip to play it"
                        .to_string(),
                );
            } else {
                self.editor.playing = true;
                self.editor.decoder_error = None;
            }
        } else {
            self.editor.playing = false;
        }
    }

    /// Toggles playback in the Video Editor, clamping to `[trim_start_ms, trim_end_ms]`.
    #[allow(dead_code)]
    pub fn toggle_play(&mut self) {
        let Some(clip) = self.selected_clip().cloned() else {
            return;
        };
        if self.editor.playing {
            self.editor.playing = false;
            if let Some(worker) = &self.worker {
                worker.send(WorkerCommand::PlayerPause);
            }
            return;
        }
        if is_sample_clip(&clip) || (self.worker.is_some() && !clip.file_path.is_file()) {
            self.editor.decoder_error = Some(
                "Sample clip has no video file on disk — record or scan a real clip to play it"
                    .to_string(),
            );
            if self.worker.is_none() {
                self.editor.playing = true;
            }
            return;
        }
        if self.editor.playhead_ms >= self.editor.trim_end_ms {
            self.editor.playhead_ms = self.editor.trim_start_ms;
            if let Some(worker) = &self.worker {
                worker.send(WorkerCommand::PlayerSeek(self.editor.trim_start_ms));
            }
        }
        self.editor.playing = true;
        self.editor.decoder_error = None;
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::PlayerPlay);
        }
    }

    /// Seeks the Video Editor playhead to `ms` (clamped to clip duration).
    #[allow(dead_code)]
    pub fn seek_editor(&mut self, ms: u64) {
        let Some(clip) = self.selected_clip().cloned() else {
            return;
        };
        let clamped = ms.min(clip.duration_ms);
        self.editor.playhead_ms = clamped;
        if let Some(worker) = &self.worker
            && !is_sample_clip(&clip)
            && clip.file_path.is_file()
        {
            worker.send(WorkerCommand::PlayerSeek(clamped));
        }
    }

    /// Seeks the Video Editor playhead relative to its current position by `delta_ms`.
    #[allow(dead_code)]
    pub fn seek_relative_ms(&mut self, delta_ms: i64) {
        let next = if delta_ms >= 0 {
            self.editor.playhead_ms.saturating_add(delta_ms as u64)
        } else {
            self.editor
                .playhead_ms
                .saturating_sub(delta_ms.unsigned_abs())
        };
        self.seek_editor(next);
    }

    /// Sets playback volume (`0.0..=1.0`) and updates the audio backend.
    #[allow(dead_code)]
    pub fn set_volume(&mut self, volume: f32) {
        let clamped = volume.clamp(0.0, 1.0);
        self.editor.volume = clamped;
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::PlayerSetVolume(clamped));
        }
    }

    /// Toggles audio mute state and updates the audio backend.
    #[allow(dead_code)]
    pub fn toggle_mute(&mut self) {
        self.editor.muted = !self.editor.muted;
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::PlayerSetMuted(self.editor.muted));
        }
    }

    /// Sets the timeline zoom factor (`1.0..=8.0`).
    #[allow(dead_code)]
    pub fn set_timeline_zoom(&mut self, zoom: f32) {
        self.editor.timeline_zoom = zoom.clamp(1.0, 8.0);
    }

    /// Restarts Video Editor playback from `trim_start_ms`.
    #[allow(dead_code)]
    pub fn restart_editor(&mut self) {
        let start = self.editor.trim_start_ms;
        self.editor.playhead_ms = start;
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::PlayerSeek(start));
        }
    }

    /// Updates `trim_start_ms` (clamped strictly below `trim_end_ms`) and notifies the decoder.
    #[allow(dead_code)]
    pub fn set_trim_start(&mut self, ms: u64) {
        let max_start = self.editor.trim_end_ms.saturating_sub(100);
        let clamped = ms.min(max_start);
        self.editor.trim_start_ms = clamped;
        if self.editor.playhead_ms < clamped {
            self.editor.playhead_ms = clamped;
        }
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::PlayerSetTrimRange(Some((
                self.editor.trim_start_ms,
                self.editor.trim_end_ms,
            ))));
        }
    }

    /// Updates `trim_end_ms` (clamped between `trim_start_ms + 100` and clip duration) and notifies the decoder.
    #[allow(dead_code)]
    pub fn set_trim_end(&mut self, ms: u64) {
        let duration_ms = self
            .selected_clip()
            .map(|c| c.duration_ms)
            .unwrap_or(ms)
            .max(100);
        let min_end = (self.editor.trim_start_ms + 100).min(duration_ms);
        let clamped = ms.clamp(min_end, duration_ms);
        self.editor.trim_end_ms = clamped;
        if self.editor.playhead_ms > clamped {
            self.editor.playhead_ms = clamped;
        }
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::PlayerSetTrimRange(Some((
                self.editor.trim_start_ms,
                self.editor.trim_end_ms,
            ))));
        }
    }

    /// Resets trim range to the full clip duration.
    #[allow(dead_code)]
    pub fn reset_trim(&mut self) {
        let duration_ms = self.selected_clip().map(|c| c.duration_ms).unwrap_or(0);
        self.editor.trim_start_ms = 0;
        self.editor.trim_end_ms = duration_ms;
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::PlayerSetTrimRange(Some((0, duration_ms))));
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

    /// Toggles favorite status for all selected clips (favorites all if any is unfavorited, else unfavorites all).
    pub fn toggle_favorite_bulk(&mut self, ids: Vec<ClipId>) {
        if ids.is_empty() {
            return;
        }
        let target_fav = ids.iter().any(|id| !self.favorite_ids.contains(id));
        for id in &ids {
            if target_fav {
                self.favorite_ids.insert(*id);
            } else {
                self.favorite_ids.remove(id);
            }
        }
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::SetFavoriteBulk {
                ids,
                favorite: target_fav,
            });
        } else {
            self.status_banner = Some(if target_fav {
                "Added selected clips to Favorites".to_string()
            } else {
                "Removed selected clips from Favorites".to_string()
            });
        }
    }

    /// Requests confirmation before moving clips to Trash.
    pub fn request_move_to_trash(&mut self, ids: Vec<ClipId>) {
        self.context_menu = None;
        let trashable: Vec<ClipId> = ids
            .into_iter()
            .filter(|id| {
                self.find_clip(*id).is_some_and(|c| {
                    !matches!(
                        self.clip_origin(c),
                        ClipOrigin::Sample | ClipOrigin::Imported
                    )
                })
            })
            .collect();
        if trashable.is_empty() {
            self.status_banner = Some(
                "Sample or external imported clips cannot be moved to Trash — use Delete Permanently to remove from library"
                    .to_string(),
            );
            return;
        }
        self.pending_confirm = Some(ConfirmAction::MoveToTrash(trashable));
    }

    /// Requests confirmation before restoring clips from Trash.
    pub fn request_restore_from_trash(&mut self, ids: Vec<ClipId>) {
        self.context_menu = None;
        if ids.is_empty() {
            return;
        }
        self.pending_confirm = Some(ConfirmAction::RestoreFromTrash(ids));
    }

    /// Requests confirmation before permanently deleting clips.
    pub fn request_permanent_delete(&mut self, ids: Vec<ClipId>) {
        self.context_menu = None;
        if ids.is_empty() {
            return;
        }
        self.pending_confirm = Some(ConfirmAction::PermanentlyDelete(ids));
    }

    /// Cancels any pending trash/restore/delete confirmation.
    pub fn cancel_confirmation(&mut self) {
        self.pending_confirm = None;
    }

    /// Executes the pending trash/restore/delete confirmation and updates UI & stats immediately.
    pub fn confirm_pending_action(&mut self) {
        let Some(action) = self.pending_confirm.take() else {
            return;
        };
        match action {
            ConfirmAction::MoveToTrash(ids) => {
                self.close_active_player_if_affected(&ids);
                let id_set: HashSet<ClipId> = ids.iter().copied().collect();
                let mut remaining = Vec::with_capacity(self.clips.len());
                let now = Utc::now();
                let mut moved_count = 0usize;
                for clip in self.clips.drain(..) {
                    if id_set.contains(&clip.id) {
                        self.trashed_at.insert(clip.id, now);
                        self.trashed_clips.push(clip);
                        moved_count += 1;
                    } else {
                        remaining.push(clip);
                    }
                }
                self.clips = remaining;
                for id in &ids {
                    self.selected_clip_ids.remove(id);
                }
                self.ensure_valid_selection_after_removal();
                if let Some(worker) = &self.worker {
                    worker.send(WorkerCommand::MoveToTrash {
                        ids,
                        managed_roots: vec![self.config.clips_dir.clone()],
                    });
                } else {
                    self.status_banner = Some(format!("Moved {moved_count} clip(s) to Trash"));
                }
            }
            ConfirmAction::RestoreFromTrash(ids) => {
                let id_set: HashSet<ClipId> = ids.iter().copied().collect();
                let mut remaining_trash = Vec::with_capacity(self.trashed_clips.len());
                let mut restored_count = 0usize;
                for clip in self.trashed_clips.drain(..) {
                    if id_set.contains(&clip.id) {
                        self.trashed_at.remove(&clip.id);
                        self.clips.push(clip);
                        restored_count += 1;
                    } else {
                        remaining_trash.push(clip);
                    }
                }
                self.trashed_clips = remaining_trash;
                for id in &ids {
                    self.selected_clip_ids.remove(id);
                }
                if let Some(worker) = &self.worker {
                    worker.send(WorkerCommand::RestoreFromTrash { ids });
                } else {
                    self.status_banner =
                        Some(format!("Restored {restored_count} clip(s) from Trash"));
                }
            }
            ConfirmAction::PermanentlyDelete(ids) => {
                self.close_active_player_if_affected(&ids);
                let id_set: HashSet<ClipId> = ids.iter().copied().collect();
                let before = self.clips.len() + self.trashed_clips.len();
                self.clips.retain(|c| !id_set.contains(&c.id));
                self.trashed_clips.retain(|c| !id_set.contains(&c.id));
                let after = self.clips.len() + self.trashed_clips.len();
                let deleted_count = before.saturating_sub(after);
                for id in &ids {
                    self.favorite_ids.remove(id);
                    self.selected_clip_ids.remove(id);
                    self.library_thumbs.remove(id);
                    self.trashed_at.remove(id);
                    self.clip_origins.remove(id);
                }
                self.ensure_valid_selection_after_removal();
                if let Some(worker) = &self.worker {
                    worker.send(WorkerCommand::PermanentlyDelete {
                        ids,
                        managed_roots: vec![self.config.clips_dir.clone()],
                    });
                } else {
                    self.status_banner =
                        Some(format!("Permanently deleted {deleted_count} clip(s)"));
                }
            }
        }
    }

    fn close_active_player_if_affected(&mut self, ids: &[ClipId]) {
        if let Some(sel_id) = self.editor.selected_clip_id
            && ids.contains(&sel_id)
        {
            self.editor.playing = false;
            self.editor.has_video_frame = false;
            self.editor.has_audio = false;
            self.editor.audio_error = None;
            self.editor.decoder_error = None;
            self.editor.timeline_thumbs.clear();
            self.editor.selected_clip_id = None;
            if let Some(worker) = &self.worker {
                worker.send(WorkerCommand::ClosePlayer);
            }
        }
    }

    fn ensure_valid_selection_after_removal(&mut self) {
        if let Some(sel_id) = self.editor.selected_clip_id
            && !self.clips.iter().any(|c| c.id == sel_id)
        {
            self.editor.selected_clip_id = None;
        }
    }

    /// Starts inline rename for the given clip.
    pub fn begin_rename(&mut self, id: ClipId) {
        self.context_menu = None;
        let Some(title) = self.find_clip(id).map(|c| c.title.clone()) else {
            return;
        };
        self.renaming_clip_id = Some(id);
        let mut edit = rune::TextEdit::new(title);
        edit.focused = true;
        self.rename_edit = edit;
    }

    /// Cancels inline rename without saving.
    pub fn cancel_rename(&mut self) {
        self.renaming_clip_id = None;
        self.rename_edit = rune::TextEdit::default();
    }

    /// Commits the inline rename to state and SQLite.
    pub fn commit_rename(&mut self) {
        let Some(id) = self.renaming_clip_id else {
            return;
        };
        let trimmed = self.rename_edit.text.trim().to_string();
        if trimmed.is_empty() {
            self.status_banner = Some("Clip title cannot be empty".to_string());
            return;
        }
        if let Some(clip) = self
            .clips
            .iter_mut()
            .chain(self.trashed_clips.iter_mut())
            .find(|c| c.id == id)
        {
            clip.title = trimmed.clone();
        }
        self.renaming_clip_id = None;
        self.rename_edit = rune::TextEdit::default();
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::RenameClip { id, title: trimmed });
        } else {
            self.status_banner = Some(format!("Renamed clip to \"{trimmed}\""));
        }
    }

    /// Opens the clip's containing folder in the system file manager.
    pub fn open_containing_folder(&mut self, id: ClipId) {
        self.context_menu = None;
        let Some(clip) = self.find_clip(id).cloned() else {
            return;
        };
        if is_sample_clip(&clip) {
            self.status_banner = Some("Sample clips do not have a folder on disk".to_string());
            return;
        }
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::OpenContainingFolder(clip.file_path));
        } else {
            let valid = clip.file_path.parent().is_some_and(|p| p.is_dir());
            self.status_banner = Some(if valid {
                format!("Opened folder for {}", clip.title)
            } else {
                format!(
                    "Containing folder does not exist: {}",
                    clip.file_path.display()
                )
            });
        }
    }

    /// Opens the floating context menu for a clip at window coordinates `(x, y)`.
    pub fn open_context_menu(&mut self, clip_id: ClipId, x: f32, y: f32) {
        let is_trashed = self.trashed_clips.iter().any(|c| c.id == clip_id);
        self.context_menu = Some(LibraryContextMenu {
            clip_id,
            x,
            y,
            is_trashed,
        });
    }

    /// Closes any open context menu.
    pub fn close_context_menu(&mut self) {
        self.context_menu = None;
    }

    /// Returns filtered and sorted clips for the Clip Library view.
    pub fn filtered_clips(&self) -> Vec<&ClipMetadata> {
        let query = self.library_search.text.trim().to_ascii_lowercase();
        let source = if self.library_filter == LibraryFilter::Trash {
            &self.trashed_clips
        } else {
            &self.clips
        };
        let mut list: Vec<&ClipMetadata> = source
            .iter()
            .filter(|c| match self.library_filter {
                LibraryFilter::All | LibraryFilter::Trash => true,
                LibraryFilter::Recorded => self.clip_origin(c) == ClipOrigin::Recorded,
                LibraryFilter::Trimmed => self.clip_origin(c) == ClipOrigin::Trimmed,
                LibraryFilter::Favorites => self.favorite_ids.contains(&c.id),
                LibraryFilter::Shared => c.cloud_share.is_some(),
            })
            .filter(|c| match &self.library_game_filter {
                Some(game) => c.game.as_ref().is_some_and(|g| &g.name == game),
                None => true,
            })
            .filter(|c| {
                if query.is_empty() {
                    return true;
                }
                c.title.to_ascii_lowercase().contains(&query)
                    || c.codec.to_ascii_lowercase().contains(&query)
                    || c.game
                        .as_ref()
                        .is_some_and(|g| g.name.to_ascii_lowercase().contains(&query))
                    || c.tags
                        .iter()
                        .any(|t| t.to_ascii_lowercase().contains(&query))
            })
            .collect();

        match self.library_sort {
            ClipSort::CreatedDesc => list.sort_by(|a, b| {
                b.created_at
                    .cmp(&a.created_at)
                    .then_with(|| a.id.0.cmp(&b.id.0))
            }),
            ClipSort::CreatedAsc => list.sort_by(|a, b| {
                a.created_at
                    .cmp(&b.created_at)
                    .then_with(|| a.id.0.cmp(&b.id.0))
            }),
            ClipSort::DurationDesc => list.sort_by(|a, b| {
                b.duration_ms
                    .cmp(&a.duration_ms)
                    .then_with(|| b.created_at.cmp(&a.created_at))
            }),
            ClipSort::DurationAsc => list.sort_by(|a, b| {
                a.duration_ms
                    .cmp(&b.duration_ms)
                    .then_with(|| b.created_at.cmp(&a.created_at))
            }),
            ClipSort::SizeDesc => list.sort_by(|a, b| {
                b.size_bytes
                    .cmp(&a.size_bytes)
                    .then_with(|| b.created_at.cmp(&a.created_at))
            }),
            ClipSort::TitleAsc => list.sort_by(|a, b| {
                a.title
                    .to_ascii_lowercase()
                    .cmp(&b.title.to_ascii_lowercase())
                    .then_with(|| b.created_at.cmp(&a.created_at))
            }),
        }

        list
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

    /// Dispatches a logical [`HotkeyAction`] using the exact same capture commands as UI buttons.
    pub fn dispatch_hotkey_action(&mut self, action: HotkeyAction) {
        match action {
            HotkeyAction::SaveReplay => self.trigger_save_replay(),
            HotkeyAction::ToggleReplayBuffer => self.toggle_replay_buffer(),
            HotkeyAction::ToggleRecording => self.toggle_recording(),
        }
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
                let msg =
                    "Sample clip cannot be exported — record or scan a real clip first".to_string();
                self.editor.exporting = false;
                self.editor.export_error = Some(msg.clone());
                self.status_banner = Some(msg);
                return;
            }
            if let Err(err) = req.validate() {
                let msg = format!("Invalid trim range: {err}");
                self.editor.exporting = false;
                self.editor.export_error = Some(msg.clone());
                self.status_banner = Some(msg);
                return;
            }
            if self.editor.playing {
                self.editor.playing = false;
                worker.send(WorkerCommand::PlayerPause);
            }
            self.editor.exporting = true;
            self.editor.export_progress = Some(0.0);
            self.editor.export_error = None;
            self.status_banner = Some("Exporting trimmed clip...".to_string());
            worker.send(WorkerCommand::TrimClip(req));
        } else {
            match req.validate() {
                Ok(()) => {
                    self.editor.playing = false;
                    self.editor.exporting = false;
                    self.editor.export_progress = None;
                    self.editor.export_error = None;
                    self.editor.last_export_path = Some(req.output_path.clone());
                    self.status_banner =
                        Some(format!("Trim validated: {}", req.output_path.display()));
                }
                Err(err) => {
                    let msg = format!("Trim error: {err}");
                    self.editor.exporting = false;
                    self.editor.export_progress = None;
                    self.editor.export_error = Some(msg.clone());
                    self.status_banner = Some(msg);
                }
            }
        }
    }

    /// Cancels an active trim export, if any.
    #[allow(dead_code)]
    pub fn cancel_export(&mut self) {
        if !self.editor.exporting {
            return;
        }
        self.editor.exporting = false;
        self.editor.export_progress = None;
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::CancelTrim);
        }
        self.status_banner = Some("Trim export cancelled".to_string());
    }

    /// Queues a clip upload entry in the Uploads view and dispatches `HttpCloudClient` upload.
    pub fn queue_upload(&mut self, id: ClipId) {
        let Some(clip) = self.clips.iter().find(|c| c.id == id).cloned() else {
            return;
        };
        if let Some(existing) = self.uploads.iter().find(|u| u.clip_id == id) {
            match &existing.status {
                UploadStatus::Queued | UploadStatus::Uploading { .. } => {
                    self.status_banner =
                        Some(format!("Upload already in progress for \"{}\"", clip.title));
                    self.nav = NavSection::Uploads;
                    return;
                }
                UploadStatus::Completed => {
                    self.status_banner = Some(format!("\"{}\" is already uploaded", clip.title));
                    self.selected_upload_id = Some(id);
                    self.nav = NavSection::Uploads;
                    return;
                }
                UploadStatus::Failed(_) | UploadStatus::Cancelled => {
                    self.retry_upload(id);
                    self.nav = NavSection::Uploads;
                    return;
                }
            }
        }
        if is_sample_clip(&clip) || !clip.file_path.is_file() {
            self.status_banner = Some(
                "Sample clip cannot be uploaded — record or scan a real clip first".to_string(),
            );
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

    /// Sets public/private visibility for a remote clip (confirmed by server response).
    pub fn set_clip_visibility(&mut self, clip_id: ClipId, make_public: bool) {
        let target_vis = if make_public {
            ShareVisibility::Public
        } else {
            ShareVisibility::Private
        };
        if let Some(worker) = &self.worker {
            self.status_banner = Some("Updating clip visibility...".to_string());
            worker.send(WorkerCommand::SetShareVisibility {
                clip_id,
                visibility: target_vis,
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
            self.apply_worker_event(ev);
        }
        changed
    }

    /// Applies a single [`WorkerEvent`] to application state.
    pub fn apply_worker_event(&mut self, ev: WorkerEvent) {
        match ev {
            WorkerEvent::DependenciesProbed(deps) => {
                self.deps = deps;
            }
            WorkerEvent::LibraryLoaded {
                clips,
                trashed_clips,
                favorites,
                origins,
                trashed_at,
            } => {
                if !clips.is_empty() || !trashed_clips.is_empty() || !self.using_sample_clips {
                    self.using_sample_clips = false;
                    self.clips = clips;
                    self.trashed_clips = trashed_clips;
                    self.favorite_ids = favorites;
                    self.clip_origins = origins;
                    self.trashed_at = trashed_at;
                    self.uploads.retain(|u| {
                        u.share
                            .as_ref()
                            .is_none_or(|sh| sh.share_id != "shr_mirage_01")
                    });
                    let active_ids: HashSet<ClipId> = self
                        .clips
                        .iter()
                        .chain(self.trashed_clips.iter())
                        .map(|c| c.id)
                        .collect();
                    self.selected_clip_ids.retain(|id| active_ids.contains(id));

                    let selected_valid = self
                        .editor
                        .selected_clip_id
                        .is_some_and(|id| self.clips.iter().any(|c| c.id == id));
                    if !selected_valid {
                        if let Some(first) = self.clips.first() {
                            self.editor.selected_clip_id = Some(first.id);
                            self.editor.trim_start_ms = 0;
                            self.editor.trim_end_ms = first.duration_ms;
                        } else {
                            self.editor.selected_clip_id = None;
                            self.editor.playing = false;
                            self.editor.has_video_frame = false;
                        }
                    }
                    if let Some(worker) = &self.worker {
                        for clip in self.clips.iter().chain(self.trashed_clips.iter()) {
                            if !self.library_thumbs.contains_key(&clip.id)
                                && let Some(thumb_path) = &clip.thumbnail_path
                                && thumb_path.is_file()
                            {
                                let pixels = rune::Pixels::new();
                                self.library_thumbs.insert(clip.id, pixels.clone());
                                worker.send(WorkerCommand::LoadThumbnail {
                                    path: thumb_path.clone(),
                                    pixels,
                                });
                            }
                        }
                    }
                }
            }
            WorkerEvent::CaptureStatusChanged(status) => {
                if matches!(
                    status,
                    CaptureStatus::Idle | CaptureStatus::ReplayBuffering { .. }
                ) {
                    self.audio_config_dirty_while_active = false;
                }
                self.capture_status = status;
            }
            WorkerEvent::HotkeyStatusChanged(status) => {
                self.hotkey_status = status;
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
            WorkerEvent::TrimProgress(progress) => {
                if self.editor.exporting {
                    self.editor.export_progress = Some(progress.clamp(0.0, 1.0));
                }
            }
            WorkerEvent::TrimCompleted(Ok(path)) => {
                self.editor.exporting = false;
                self.editor.export_progress = None;
                self.editor.export_error = None;
                self.editor.last_export_path = Some(path.clone());
                self.status_banner = Some(format!("Trimmed clip exported: {}", path.display()));
            }
            WorkerEvent::TrimCompleted(Err(err)) => {
                self.editor.exporting = false;
                self.editor.export_progress = None;
                if err.to_ascii_lowercase().contains("cancel") {
                    self.editor.export_error = None;
                    self.status_banner = Some("Trim export cancelled".to_string());
                } else {
                    self.editor.export_error = Some(err.clone());
                    self.status_banner = Some(format!("Trim failed: {err}"));
                }
            }
            WorkerEvent::PlayerFrameDelivered {
                clip_id,
                width,
                height,
                pts_ms,
            } => {
                if self.editor.selected_clip_id == Some(clip_id) {
                    self.editor.has_video_frame = true;
                    self.editor.video_width = width;
                    self.editor.video_height = height;
                    self.editor.playhead_ms = pts_ms;
                }
            }
            WorkerEvent::PlayerStateChanged { clip_id, playing } => {
                if self.editor.selected_clip_id == Some(clip_id) {
                    self.editor.playing = playing;
                }
            }
            WorkerEvent::PlayerPosition { clip_id, pts_ms } => {
                if self.editor.selected_clip_id == Some(clip_id) {
                    self.editor.playhead_ms = pts_ms;
                }
            }
            WorkerEvent::PlayerAudioStatus {
                clip_id,
                has_audio,
                error,
            } => {
                if self.editor.selected_clip_id == Some(clip_id) {
                    self.editor.has_audio = has_audio;
                    self.editor.audio_error = error;
                }
            }
            WorkerEvent::PlayerError { clip_id, message } => {
                if self.editor.selected_clip_id == Some(clip_id) {
                    self.editor.playing = false;
                    self.editor.decoder_error = Some(message);
                }
            }
            WorkerEvent::TimelineThumbsLoaded { clip_id, thumbs } => {
                if self.editor.selected_clip_id == Some(clip_id) {
                    self.editor.timeline_thumbs = thumbs;
                }
            }
            WorkerEvent::UploadProgress {
                clip_id,
                progress_pct,
            } => {
                if let Some(entry) = self.uploads.iter_mut().find(|u| u.clip_id == clip_id)
                    && !matches!(
                        entry.status,
                        UploadStatus::Cancelled | UploadStatus::Completed
                    )
                {
                    entry.status = UploadStatus::Uploading { progress_pct };
                }
            }
            WorkerEvent::UploadCancelled { clip_id } => {
                if let Some(entry) = self.uploads.iter_mut().find(|u| u.clip_id == clip_id)
                    && !matches!(entry.status, UploadStatus::Completed)
                {
                    entry.status = UploadStatus::Cancelled;
                    self.status_banner = Some("Upload cancelled (local clip kept)".to_string());
                }
            }
            WorkerEvent::UploadCompleted { clip_id, result } => {
                if let Some(entry) = self.uploads.iter_mut().find(|u| u.clip_id == clip_id) {
                    match result {
                        Ok(summary) => {
                            entry.status = UploadStatus::Completed;
                            entry.is_public = summary.visibility == ShareVisibility::Public;
                            entry.share = Some(summary.clone().into());
                            if !self
                                .remote_shares
                                .iter()
                                .any(|r| r.clip_id == summary.clip_id)
                            {
                                self.remote_shares.insert(0, summary);
                            }
                            self.status_banner = Some("Clip uploaded successfully".to_string());
                        }
                        Err(err) => {
                            entry.status = UploadStatus::Failed(err.clone());
                            self.status_banner = Some(format!("Upload failed: {err}"));
                        }
                    }
                }
            }
            WorkerEvent::VisibilityUpdated { clip_id, result } => match result {
                Ok(summary) => {
                    let is_pub = summary.visibility == ShareVisibility::Public;
                    if let Some(entry) = self.uploads.iter_mut().find(|u| u.clip_id == clip_id) {
                        entry.is_public = is_pub;
                        if !summary.share_id.is_empty() {
                            entry.share = Some(summary.clone().into());
                        }
                    }
                    if let Some(remote) =
                        self.remote_shares.iter_mut().find(|r| r.clip_id == clip_id)
                    {
                        *remote = summary;
                    }
                    self.server_error = None;
                    self.status_banner = Some(format!(
                        "Visibility updated to {}",
                        if is_pub { "Public" } else { "Private" }
                    ));
                }
                Err(err) => {
                    self.server_error = Some(err.clone());
                    self.status_banner = Some(format!("Visibility update failed: {err}"));
                }
            },
            WorkerEvent::ShareCreated { clip_id, result } => match result {
                Ok(resp) => {
                    let info = CloudShareInfo {
                        share_id: resp.share_id.clone(),
                        url: resp.url.clone(),
                        uploaded_at: resp.created_at,
                        expires_at: resp.expires_at,
                    };
                    if let Some(entry) = self.uploads.iter_mut().find(|u| u.clip_id == clip_id) {
                        entry.share = Some(info.clone());
                        entry.is_public = resp.visibility == ShareVisibility::Public;
                    }
                    if let Some(remote) =
                        self.remote_shares.iter_mut().find(|r| r.clip_id == clip_id)
                    {
                        remote.share_id = resp.share_id;
                        remote.url = resp.url;
                        remote.visibility = resp.visibility;
                        remote.expires_at = resp.expires_at;
                    }
                    self.status_banner = Some("Share link created".to_string());
                }
                Err(err) => {
                    self.server_error = Some(err.clone());
                    self.status_banner = Some(format!("Create share failed: {err}"));
                }
            },
            WorkerEvent::ShareRevoked {
                clip_id,
                share_id,
                result,
            } => match result {
                Ok(()) => {
                    if let Some(entry) = self.uploads.iter_mut().find(|u| u.clip_id == clip_id) {
                        entry.share = None;
                        entry.is_public = false;
                    }
                    if let Some(remote) = self
                        .remote_shares
                        .iter_mut()
                        .find(|r| r.clip_id == clip_id || r.share_id == share_id)
                    {
                        remote.share_id.clear();
                        remote.url.clear();
                        remote.visibility = ShareVisibility::Private;
                    }
                    self.status_banner = Some("Share link revoked".to_string());
                }
                Err(err) => {
                    self.server_error = Some(err.clone());
                    self.status_banner = Some(format!("Revoke share failed: {err}"));
                }
            },
            WorkerEvent::CaptureSourcesDiscovered {
                monitors,
                audio_devices,
                app_audio,
            } => {
                self.available_monitors = monitors;
                self.available_audio_devices = audio_devices;
                self.available_app_audio = app_audio;
            }
            WorkerEvent::ServerHealthChecked {
                ok,
                error,
                server_info,
            } => {
                self.server_health_ok = Some(ok);
                self.server_error = error;
                if let Some(info) = server_info {
                    self.server_info = Some(info);
                }
            }
            WorkerEvent::ServerMetadataLoaded {
                server_info,
                current_user,
                quota,
            } => {
                if let Some(info) = server_info {
                    self.server_info = Some(info);
                }
                if let Some(user) = current_user {
                    self.current_user = Some(user);
                }
                if let Some(q) = quota {
                    self.quota = Some(q);
                }
            }
            WorkerEvent::ServerConnected(res) => match res {
                Ok((url, token)) => {
                    self.config.server_url = Some(url.clone());
                    if let Some(tok) = token {
                        self.config.api_token = Some(tok.clone());
                        self.api_token_edit = rune::TextEdit::new(tok);
                    }
                    self.server_url_edit = rune::TextEdit::new(url.clone());
                    self.server_connected = true;
                    self.server_health_ok = Some(true);
                    self.server_error = None;
                    self.save_config();
                    self.refresh_remote_shares();
                    self.status_banner = Some(format!("Connected to {url}"));
                }
                Err(err) => {
                    self.server_connected = false;
                    self.server_health_ok = Some(false);
                    self.server_error = Some(err.clone());
                    self.status_banner = Some(format!("Server connection failed: {err}"));
                }
            },
            WorkerEvent::RemoteSharesLoaded(res) => match res {
                Ok(list) => {
                    for entry in &mut self.uploads {
                        if let Some(remote) = list.iter().find(|r| r.clip_id == entry.clip_id) {
                            entry.is_public = remote.visibility == ShareVisibility::Public;
                            entry.share = if remote.share_id.is_empty() {
                                None
                            } else {
                                Some(remote.clone().into())
                            };
                        }
                    }
                    self.remote_shares = list;
                    self.server_error = None;
                }
                Err(err) => {
                    self.server_error = Some(err);
                }
            },
            WorkerEvent::RemoteShareDeleted { clip_id, result } => {
                self.confirm_remote_delete = None;
                match result {
                    Ok(()) => {
                        self.remote_shares.retain(|s| s.clip_id != clip_id);
                        self.uploads.retain(|u| u.clip_id != clip_id);
                        if self.selected_upload_id == Some(clip_id) {
                            self.selected_upload_id = None;
                        }
                        self.server_error = None;
                        self.status_banner = Some("Deleted clip from remote server".to_string());
                    }
                    Err(err) => {
                        self.server_error = Some(err.clone());
                        self.status_banner = Some(format!("Remote delete failed: {err}"));
                    }
                }
            }
            WorkerEvent::MicTest(ev) => match ev {
                MicTestEvent::PhaseChanged(phase) => {
                    self.mic_test.phase = phase;
                    if matches!(phase, MicTestPhase::Recording) {
                        self.mic_test.error = None;
                    }
                }
                MicTestEvent::Level {
                    rms_dbfs,
                    peak_dbfs,
                    elapsed_ms,
                } => {
                    self.mic_test.rms_dbfs = rms_dbfs;
                    self.mic_test.peak_dbfs = peak_dbfs;
                    self.mic_test.elapsed_ms = elapsed_ms;
                }
                MicTestEvent::Recorded { summary } => {
                    self.mic_test.rms_dbfs = summary.rms_dbfs;
                    self.mic_test.peak_dbfs = summary.peak_dbfs;
                    self.mic_test.feedback = Some(summary.feedback);
                    self.mic_test.error = None;
                    self.mic_test.diagnostic_summary = Some(summary);
                }
                MicTestEvent::PlaybackFinished => {
                    self.mic_test.phase = MicTestPhase::Completed;
                }
                MicTestEvent::Error {
                    feedback,
                    message,
                    summary,
                } => {
                    self.mic_test.phase = MicTestPhase::Error;
                    self.mic_test.feedback = Some(feedback);
                    self.mic_test.error = Some(message);
                    if summary.is_some() {
                        self.mic_test.diagnostic_summary = summary;
                    }
                }
            },
            WorkerEvent::Banner(msg) => {
                self.status_banner = Some(msg);
            }
        }
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

#[allow(dead_code)]
impl AppState {
    /// Sets the Library game filter (`None` for all games) and clears invalid selection.
    pub fn set_library_game_filter(&mut self, game: Option<String>) {
        self.library_game_filter = game;
    }

    /// Returns distinct sorted game names present in the active library.
    pub fn available_games(&self) -> Vec<String> {
        let mut games: Vec<String> = self
            .clips
            .iter()
            .filter_map(|c| c.game.as_ref().map(|g| g.name.clone()))
            .collect();
        games.sort();
        games.dedup();
        games
    }

    /// Selects the active audio track index in the Video Editor (`None` = default/first).
    pub fn set_audio_track(&mut self, track: Option<usize>) {
        self.editor.audio_track = track;
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::SetAudioTrack(track));
        }
    }

    /// Requests live monitor, audio device, and app audio discovery from `gpu-screen-recorder`.
    pub fn discover_capture_sources(&mut self) {
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::DiscoverCaptureSources);
        }
    }

    /// Sets capture resolution (`None, None` = native).
    pub fn set_capture_resolution(&mut self, width: Option<u32>, height: Option<u32>) {
        self.config.capture.width = width;
        self.config.capture.height = height;
    }

    /// Sets capture frame rate.
    pub fn set_capture_fps(&mut self, fps: u32) {
        self.config.capture.fps = fps.clamp(15, 240);
    }

    /// Sets capture video codec.
    pub fn set_video_codec(&mut self, codec: VideoCodec) {
        self.config.capture.video_codec = codec;
    }

    /// Sets capture audio codec.
    pub fn set_audio_codec(&mut self, codec: AudioCodec) {
        self.config.capture.audio_codec = codec;
    }

    /// Sets replay buffer duration in seconds.
    pub fn set_replay_buffer_secs(&mut self, secs: u32) {
        self.config.capture.replay_buffer_secs = secs.clamp(5, 600);
    }

    /// Toggles an audio source entry in `config.capture.audio_sources` (matching by raw ID).
    pub fn toggle_audio_source(&mut self, source: String) {
        let target_id = parse_routed_audio_source(&source)
            .map(|(_, id)| id)
            .unwrap_or(source.clone());
        if let Some(pos) = self.config.capture.audio_sources.iter().position(|raw| {
            parse_routed_audio_source(raw)
                .map(|(_, id)| id == target_id)
                .unwrap_or_else(|_| raw == &target_id)
        }) {
            self.config.capture.audio_sources.remove(pos);
        } else {
            self.config.capture.audio_sources.push(source);
        }
        if !matches!(self.capture_status, CaptureStatus::Idle) {
            self.audio_config_dirty_while_active = true;
        }
    }

    /// Sets whether replay buffering restarts automatically after saving.
    pub fn set_restart_replay_on_save(&mut self, enabled: bool) {
        self.config.capture.restart_replay_on_save = enabled;
    }

    /// Sets whether Wayland screencast portal session tokens are restored.
    pub fn set_restore_portal_session(&mut self, enabled: bool) {
        self.config.capture.restore_portal_session = enabled;
    }

    /// Updates `clips_dir` and persists settings.
    pub fn set_clips_dir(&mut self, path: PathBuf) {
        if !path.as_os_str().is_empty() {
            self.clips_dir_edit = rune::TextEdit::new(path.display().to_string());
            self.config.clips_dir = path;
        }
    }

    /// Syncs editable Settings fields (`clips_dir_edit`, `hotkeys`, `server_url_edit`, `api_token_edit`)
    /// into `AppConfig` and persists `config.json`.
    pub fn save_settings(&mut self) {
        let dir_text = self.clips_dir_edit.text.trim();
        if !dir_text.is_empty() {
            self.config.clips_dir = PathBuf::from(dir_text);
        }
        let url_text = self.server_url_edit.text.trim();
        self.config.server_url = if url_text.is_empty() {
            None
        } else {
            Some(url_text.trim_end_matches('/').to_string())
        };
        let tok_text = self.api_token_edit.text.trim();
        self.config.api_token = if tok_text.is_empty() {
            if let Some(url) = self.config.server_url.as_deref() {
                let _ = delete_api_token(url);
            }
            None
        } else {
            if let Some(url) = self.config.server_url.as_deref() {
                let _ = store_api_token(url, tok_text);
            }
            Some(tok_text.to_string())
        };
        self.config.hotkeys = HotkeyBindings {
            save_replay: self.hotkeys.save_replay.clone(),
            toggle_replay: self.hotkeys.toggle_replay.clone(),
            toggle_record: self.hotkeys.toggle_record.clone(),
        };
        self.save_config();
    }

    /// Resets `AppConfig` to defaults (preserving worker) and updates editable fields.
    pub fn reset_settings(&mut self) {
        if let Some(url) = self.config.server_url.as_deref() {
            let _ = delete_api_token(url);
        }
        self.config = AppConfig::default();
        self.hotkeys = HotkeyConfig::default();
        self.clips_dir_edit = rune::TextEdit::new(self.config.clips_dir.display().to_string());
        self.server_url_edit = rune::TextEdit::default();
        self.api_token_edit = rune::TextEdit::default();
        self.server_connected = false;
        self.server_health_ok = None;
        self.server_error = None;
        self.server_info = None;
        self.current_user = None;
        self.quota = None;
        self.save_config();
    }

    /// Checks cloud server `/health` endpoint.
    pub fn check_server_health(&mut self) {
        let url = self.server_url_edit.text.trim().to_string();
        let target = if url.is_empty() {
            self.config.server_url.clone().unwrap_or_default()
        } else {
            url
        };
        if target.is_empty() {
            self.server_health_ok = Some(false);
            self.server_error = Some("Enter a server URL first".to_string());
            return;
        }
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::CheckServerHealth { server_url: target });
        }
    }

    /// Connects (and optionally logs in or registers) to the configured ClipForge server.
    pub fn connect_server(&mut self, register: bool) {
        let url = self.server_url_edit.text.trim().to_string();
        let token = {
            let t = self.api_token_edit.text.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };
        let user = {
            let u = self.server_user_edit.text.trim();
            if u.is_empty() {
                None
            } else {
                Some(u.to_string())
            }
        };
        let pass = {
            let p = self.server_pass_edit.text.trim();
            if p.is_empty() {
                None
            } else {
                Some(p.to_string())
            }
        };
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::ConnectServer {
                server_url: url,
                api_token: token,
                username: user,
                password: pass,
                register,
            });
        }
    }

    /// Disconnects from the cloud server and clears cached remote state.
    pub fn disconnect_server(&mut self) {
        if let Some(url) = self.config.server_url.as_deref() {
            let _ = delete_api_token(url);
        }
        self.server_connected = false;
        self.server_health_ok = None;
        self.server_error = None;
        self.server_info = None;
        self.current_user = None;
        self.quota = None;
        self.remote_shares.clear();
        self.confirm_remote_delete = None;
        self.config.api_token = None;
        self.api_token_edit = rune::TextEdit::default();
        self.save_config();
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::DisconnectServer);
        }
    }

    /// Refreshes the list of remote shared clips from the server.
    pub fn refresh_remote_shares(&mut self) {
        let Some(url) = self.config.server_url.clone().filter(|u| !u.is_empty()) else {
            return;
        };
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::RefreshRemoteShares {
                server_url: url,
                api_token: self.config.api_token.clone(),
            });
        }
    }

    /// Requests confirmation before deleting a remote clip.
    pub fn request_delete_remote_share(&mut self, clip_id: ClipId) {
        self.confirm_remote_delete = Some(clip_id);
    }

    /// Cancels a pending remote clip deletion.
    pub fn cancel_delete_remote_share(&mut self) {
        self.confirm_remote_delete = None;
    }

    /// Confirms and dispatches remote clip deletion.
    pub fn confirm_delete_remote_share(&mut self) {
        let Some(clip_id) = self.confirm_remote_delete.take() else {
            return;
        };
        let Some(url) = self.config.server_url.clone().filter(|u| !u.is_empty()) else {
            let err = "No server URL configured".to_string();
            self.server_error = Some(err.clone());
            self.status_banner = Some(format!("Remote delete failed: {err}"));
            return;
        };
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::DeleteRemoteShare {
                clip_id,
                server_url: url,
                api_token: self.config.api_token.clone(),
            });
        }
    }

    /// Retries a failed upload entry.
    pub fn retry_upload(&mut self, clip_id: ClipId) {
        if let Some(entry) = self.uploads.iter_mut().find(|u| u.clip_id == clip_id) {
            entry.status = UploadStatus::Queued;
        }
        if let Some(clip) = self.clips.iter().find(|c| c.id == clip_id).cloned()
            && let Some(worker) = &self.worker
        {
            let visibility = self
                .uploads
                .iter()
                .find(|u| u.clip_id == clip_id)
                .map(|u| {
                    if u.is_public {
                        ShareVisibility::Public
                    } else {
                        ShareVisibility::Private
                    }
                })
                .unwrap_or(ShareVisibility::Public);
            worker.send(WorkerCommand::UploadClip {
                clip: Box::new(clip),
                visibility,
                server_url: self.config.server_url.clone(),
                api_token: self.config.api_token.clone(),
            });
        }
    }

    /// Cancels an active or queued upload entry (or removes a cancelled/failed entry) without
    /// deleting the local clip on disk.
    pub fn cancel_upload(&mut self, clip_id: ClipId) {
        let should_dismiss = self
            .uploads
            .iter()
            .find(|u| u.clip_id == clip_id)
            .is_some_and(|u| matches!(u.status, UploadStatus::Cancelled | UploadStatus::Failed(_)));

        if should_dismiss {
            self.uploads.retain(|u| u.clip_id != clip_id);
            if self.selected_upload_id == Some(clip_id) {
                self.selected_upload_id = None;
            }
            return;
        }

        if let Some(entry) = self.uploads.iter_mut().find(|u| u.clip_id == clip_id) {
            entry.status = UploadStatus::Cancelled;
        }
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::CancelUpload(clip_id));
        }
    }

    /// Creates a share link for an uploaded remote clip.
    pub fn create_share_link(&mut self, clip_id: ClipId, is_public: bool) {
        let Some(url) = self.config.server_url.clone().filter(|u| !u.is_empty()) else {
            self.server_error = Some("No server URL configured".to_string());
            return;
        };
        let visibility = if is_public {
            ShareVisibility::Public
        } else {
            ShareVisibility::Private
        };
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::CreateShare {
                clip_id,
                visibility,
                expires_in_hours: None,
                server_url: url,
                api_token: self.config.api_token.clone(),
            });
        }
    }

    /// Revokes an existing share link without deleting the remote or local clip.
    pub fn revoke_share_link(&mut self, clip_id: ClipId, share_id: String) {
        let Some(url) = self.config.server_url.clone().filter(|u| !u.is_empty()) else {
            self.server_error = Some("No server URL configured".to_string());
            return;
        };
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::RevokeShare {
                clip_id,
                share_id,
                server_url: url,
                api_token: self.config.api_token.clone(),
            });
        }
    }

    /// Copies text to the system clipboard via the background worker.
    pub fn copy_to_clipboard(&mut self, text: String) {
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::CopyToClipboard(text));
        } else {
            self.status_banner = Some(format!("Copied: {text}"));
        }
    }

    /// Opens an `http://` or `https://` URL in the default browser.
    pub fn open_url(&mut self, url: String) {
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::OpenUrl(url));
        } else {
            self.status_banner = Some(format!("Open URL: {url}"));
        }
    }

    /// Builds a sanitized diagnostic report (never containing tokens, passwords, or S3 keys).
    pub fn build_diagnostics_report(&self) -> crate::diagnostics::DiagnosticsReport {
        let display_session = if cfg!(target_os = "windows") {
            "windows".to_string()
        } else if self.is_wayland {
            "wayland".to_string()
        } else {
            "x11".to_string()
        };
        let capture_backend = if cfg!(target_os = "windows") {
            "windows-graphics-capture + wasapi".to_string()
        } else if cfg!(target_os = "linux") {
            "gpu-screen-recorder".to_string()
        } else {
            "unsupported".to_string()
        };
        let audio_routing_summary = format!(
            "{} source(s) configured",
            self.config.capture.audio_sources.len()
        );
        let server_url_redacted = self
            .config
            .server_url
            .as_deref()
            .map(crate::diagnostics::redact_server_url)
            .filter(|s| !s.is_empty());

        crate::diagnostics::DiagnosticsReport {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            display_session,
            config_path: crate::config::config_file_path().display().to_string(),
            clips_dir: self.config.clips_dir.display().to_string(),
            capture_backend,
            capture_status: format!("{:?}", self.capture_status),
            hotkey_status: self.hotkey_status.label(),
            dependencies: self.deps.clone(),
            audio_routing_summary,
            server_url_redacted,
            api_token_configured: self
                .config
                .api_token
                .as_deref()
                .is_some_and(|t| !t.trim().is_empty()),
            generated_at: Utc::now().to_rfc3339(),
        }
    }

    /// Exports a sanitized diagnostic report via the background worker.
    pub fn export_diagnostics(&mut self) {
        let report = self.build_diagnostics_report();
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::ExportDiagnostics(report));
        } else {
            let dir = crate::config::diagnostics_dir();
            match crate::diagnostics::export_diagnostics_to_dir(&dir, &report) {
                Ok(path) => {
                    self.status_banner =
                        Some(format!("Diagnostics exported to {}", path.display()));
                }
                Err(err) => {
                    self.status_banner = Some(format!("Failed to export diagnostics: {err}"));
                }
            }
        }
    }

    /// Returns the assigned [`AudioTrackTarget`] if `source_id` is enabled in `capture.audio_sources`.
    pub fn audio_source_track(&self, source_id: &str) -> Option<AudioTrackTarget> {
        let clean = parse_routed_audio_source(source_id)
            .map(|(_, id)| id)
            .unwrap_or_else(|_| source_id.to_string());
        self.config.capture.audio_sources.iter().find_map(|raw| {
            let (target, id) = parse_routed_audio_source(raw).ok()?;
            if id == clean { Some(target) } else { None }
        })
    }

    /// Assigns `target` (`Auto`, `T1`..`T4`) to `source_id`, enabling it if not already present.
    pub fn set_audio_source_track(&mut self, source_id: &str, target: AudioTrackTarget) {
        let clean = parse_routed_audio_source(source_id)
            .map(|(_, id)| id)
            .unwrap_or_else(|_| source_id.to_string());
        let Ok(formatted) = format_routed_audio_source(target, &clean) else {
            return;
        };
        if let Some(slot) = self.config.capture.audio_sources.iter_mut().find(|raw| {
            parse_routed_audio_source(raw)
                .map(|(_, id)| id == clean)
                .unwrap_or_else(|_| *raw == &clean)
        }) {
            *slot = formatted;
        } else {
            self.config.capture.audio_sources.push(formatted);
        }
        if !matches!(self.capture_status, CaptureStatus::Idle) {
            self.audio_config_dirty_while_active = true;
        }
    }

    /// Restarts the replay buffer with the current `CaptureConfig` so audio/capture edits take effect.
    pub fn restart_replay_buffer(&mut self) {
        self.audio_config_dirty_while_active = false;
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::RestartReplayBuffer(
                self.config.capture.clone(),
            ));
        } else {
            self.capture_status = CaptureStatus::ReplayBuffering {
                started_at: Utc::now(),
                buffer_secs: self.config.capture.replay_buffer_secs,
            };
        }
    }

    /// Computes live audio routing and last-clip stream diagnostics for the Recording Studio UI.
    pub fn audio_diagnostics(&self) -> AudioDiagnosticsSnapshot {
        let routing_plan =
            resolve_audio_routing(&self.config.capture.audio_sources).unwrap_or_default();
        let last_real = self
            .clips
            .iter()
            .filter(|c| !is_sample_clip(c))
            .max_by_key(|c| c.created_at);
        let (last_saved_clip_title, last_saved_stream_count, last_saved_codec, last_saved_channels) =
            if let Some(clip) = last_real {
                let first = clip.audio_tracks.first();
                (
                    Some(clip.title.clone()),
                    clip.audio_tracks.len(),
                    first.map(|t| t.codec.clone()),
                    first.map(|t| u32::from(t.channels)),
                )
            } else {
                (None, 0, None, None)
            };
        let mic_signal_detected = self
            .mic_test
            .diagnostic_summary
            .as_ref()
            .map(|s| s.signal_detected);
        AudioDiagnosticsSnapshot {
            routing_plan,
            last_saved_clip_title,
            last_saved_stream_count,
            last_saved_codec,
            last_saved_channels,
            mic_signal_detected,
        }
    }

    /// Returns the currently selected microphone source ID, if any.
    pub fn selected_mic_source(&self) -> Option<String> {
        for raw in &self.config.capture.audio_sources {
            if let Ok((_, id)) = parse_routed_audio_source(raw)
                && is_mic_source(&id)
            {
                return Some(id);
            }
        }
        self.available_audio_devices
            .iter()
            .find(|d| is_mic_source(&d.id))
            .map(|d| d.id.clone())
    }

    /// Starts the standalone PipeWire microphone test on `selected_mic_source()`.
    pub fn start_mic_test(&mut self) {
        let Some(source_id) = self.selected_mic_source() else {
            self.mic_test.phase = MicTestPhase::Error;
            self.mic_test.feedback = Some(MicFeedbackKind::MissingDevice);
            self.mic_test.error =
                Some("No microphone source selected in Audio Sources.".to_string());
            return;
        };
        self.mic_test.selected_source = Some(source_id.clone());
        self.mic_test.phase = MicTestPhase::Recording;
        self.mic_test.rms_dbfs = DBFS_FLOOR;
        self.mic_test.peak_dbfs = DBFS_FLOOR;
        self.mic_test.elapsed_ms = 0;
        self.mic_test.feedback = None;
        self.mic_test.error = None;
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::StartMicTest(source_id));
        }
    }

    /// Stops any active microphone test recording or playback immediately.
    pub fn stop_mic_test(&mut self) {
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::StopMicTest);
        }
        if matches!(
            self.mic_test.phase,
            MicTestPhase::Recording | MicTestPhase::Playing
        ) {
            self.mic_test.phase = MicTestPhase::Idle;
        }
    }

    /// Plays back the most recently recorded microphone test sample via `cpal`.
    pub fn play_mic_test_sample(&mut self) {
        self.mic_test.phase = MicTestPhase::Playing;
        self.mic_test.error = None;
        if let Some(worker) = &self.worker {
            worker.send(WorkerCommand::PlayMicTestSample);
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
