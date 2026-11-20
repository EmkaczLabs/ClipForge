use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{ClipForgeError, Result};

/// Strongly-typed identifier for a recorded or imported clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClipId(pub Uuid);

impl ClipId {
    /// Generates a new random v4 [`ClipId`].
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Wraps an existing [`Uuid`].
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }
}

impl Default for ClipId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ClipId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for ClipId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Uuid::parse_str(s).map(Self)
    }
}

/// Metadata describing the game or application associated with a clip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameInfo {
    pub id: String,
    pub name: String,
    pub executable: Option<String>,
    pub process_name: Option<String>,
}

/// Information about a single audio track inside a media container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioTrackInfo {
    pub index: u32,
    pub codec: String,
    pub channels: u16,
    pub sample_rate: u32,
    pub label: Option<String>,
}

/// Cloud share status for an uploaded clip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudShareInfo {
    pub share_id: String,
    pub url: String,
    pub uploaded_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Full metadata record for a clip stored in the local library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipMetadata {
    pub id: ClipId,
    pub title: String,
    pub game: Option<GameInfo>,
    pub file_path: PathBuf,
    pub thumbnail_path: Option<PathBuf>,
    pub created_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub size_bytes: u64,
    pub codec: String,
    pub audio_tracks: Vec<AudioTrackInfo>,
    pub tags: Vec<String>,
    pub cloud_share: Option<CloudShareInfo>,
    pub notes: Option<String>,
}

/// Target surface/source to capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CaptureTarget {
    #[default]
    PrimaryScreen,
    Monitor(String),
    FocusedWindow,
    Portal,
    Region {
        width: u32,
        height: u32,
        x: i32,
        y: i32,
    },
}

/// Supported video codecs for capture and export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    #[default]
    Auto,
    H264,
    Hevc,
    Av1,
    Vp8,
    Vp9,
}

/// Supported audio codecs for capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AudioCodec {
    #[default]
    Opus,
    Aac,
    Flac,
}

/// Supported output container formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ContainerFormat {
    #[default]
    Mp4,
    Mkv,
    Webm,
    Flv,
}

/// Quality preset mapped to encoder quality flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum QualityPreset {
    Medium,
    #[default]
    High,
    VeryHigh,
    Ultra,
}

/// Hardware or CPU encoder selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EncoderMode {
    #[default]
    Gpu,
    Cpu,
}

/// Configuration for the capture backend (e.g., `gpu-screen-recorder`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureConfig {
    pub output_dir: PathBuf,
    pub replay_buffer_secs: u32,
    pub fps: u32,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub video_codec: VideoCodec,
    pub audio_codec: AudioCodec,
    pub quality: QualityPreset,
    pub encoder: EncoderMode,
    pub container: ContainerFormat,
    pub capture_target: CaptureTarget,
    pub audio_sources: Vec<String>,
    pub restart_replay_on_save: bool,
    pub restore_portal_session: bool,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("clips"),
            replay_buffer_secs: 60,
            fps: 60,
            width: None,
            height: None,
            video_codec: VideoCodec::Auto,
            audio_codec: AudioCodec::Opus,
            quality: QualityPreset::High,
            encoder: EncoderMode::Gpu,
            container: ContainerFormat::Mp4,
            capture_target: CaptureTarget::PrimaryScreen,
            audio_sources: vec!["default_output".to_string()],
            restart_replay_on_save: false,
            restore_portal_session: true,
        }
    }
}

/// Current operational state of the capture backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CaptureStatus {
    #[default]
    Idle,
    ReplayBuffering {
        started_at: DateTime<Utc>,
        buffer_secs: u32,
    },
    Recording {
        started_at: DateTime<Utc>,
        output_path: PathBuf,
        paused: bool,
    },
    Error {
        message: String,
    },
}

/// Asynchronous events emitted by the capture backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureEvent {
    StatusChanged(CaptureStatus),
    ReplaySaved {
        path: PathBuf,
        saved_at: DateTime<Utc>,
    },
    RecordingFinished {
        path: PathBuf,
        finished_at: DateTime<Utc>,
    },
    Warning(String),
    Error(String),
}

/// Technical media properties extracted from a video file via `ffprobe`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaProbeResult {
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub video_codec: String,
    pub size_bytes: u64,
    pub audio_tracks: Vec<AudioTrackInfo>,
}

/// Parameters for trimming/exporting a clip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrimRequest {
    pub source_path: PathBuf,
    pub output_path: PathBuf,
    pub start_ms: u64,
    pub end_ms: u64,
    pub reencode: bool,
    pub video_codec: Option<VideoCodec>,
}

impl TrimRequest {
    /// Validates that the trim range is non-empty and `start_ms < end_ms`.
    pub fn validate(&self) -> Result<()> {
        if self.start_ms >= self.end_ms {
            return Err(ClipForgeError::InvalidInput(format!(
                "trim start_ms ({}) must be strictly less than end_ms ({})",
                self.start_ms, self.end_ms
            )));
        }
        Ok(())
    }
}

/// Query filter for listing clips from [`crate::traits::ClipStore`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ClipFilter {
    pub query: Option<String>,
    pub game_id: Option<String>,
    pub tag: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Persisted global hotkey bindings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyBindings {
    pub save_replay: String,
    pub toggle_replay: String,
    pub toggle_record: String,
}

impl Default for HotkeyBindings {
    fn default() -> Self {
        Self {
            save_replay: "F9".to_string(),
            toggle_replay: "F10".to_string(),
            toggle_record: "F11".to_string(),
        }
    }
}

fn default_true() -> bool {
    true
}

/// Top-level application configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    pub clips_dir: PathBuf,
    pub database_path: PathBuf,
    pub capture: CaptureConfig,
    pub server_url: Option<String>,
    pub api_token: Option<String>,
    pub auto_generate_thumbnails: bool,
    #[serde(default)]
    pub hotkeys: HotkeyBindings,
    #[serde(default = "default_true")]
    pub confirm_trash: bool,
    #[serde(default)]
    pub start_minimized: bool,
    #[serde(default)]
    pub compact_density: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            clips_dir: PathBuf::from("clips"),
            database_path: PathBuf::from("clipforge.db"),
            capture: CaptureConfig::default(),
            server_url: None,
            api_token: None,
            auto_generate_thumbnails: true,
            hotkeys: HotkeyBindings::default(),
            confirm_trash: true,
            start_minimized: false,
            compact_density: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_id_roundtrip() {
        let id = ClipId::new();
        let parsed: ClipId = id.to_string().parse().expect("valid uuid");
        assert_eq!(id, parsed);
    }

    #[test]
    fn capture_config_json_roundtrip() {
        let cfg = CaptureConfig::default();
        let json = serde_json::to_string(&cfg).expect("serialize");
        let decoded: CaptureConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cfg, decoded);
    }

    #[test]
    fn trim_request_validation() {
        let valid = TrimRequest {
            source_path: PathBuf::from("in.mp4"),
            output_path: PathBuf::from("out.mp4"),
            start_ms: 1_000,
            end_ms: 5_000,
            reencode: false,
            video_codec: None,
        };
        assert!(valid.validate().is_ok());

        let invalid = TrimRequest {
            start_ms: 5_000,
            end_ms: 5_000,
            ..valid
        };
        assert!(invalid.validate().is_err());
    }
}
