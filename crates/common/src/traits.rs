use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::error::Result;
use crate::models::{
    CaptureConfig, CaptureEvent, CaptureStatus, ClipFilter, ClipId, ClipMetadata, CloudShareInfo,
    MediaProbeResult, TrimRequest,
};

/// Abstraction over screen/game capture & replay buffer backends.
#[async_trait]
pub trait CaptureBackend: Send + Sync {
    /// Returns `true` if the underlying capture binary/service is available on this host.
    async fn is_available(&self) -> bool;

    /// Starts continuous replay buffering with the given configuration.
    async fn start_replay_buffer(&self, config: &CaptureConfig) -> Result<()>;

    /// Triggers saving the current replay buffer to disk and returns the saved file path.
    async fn save_replay(&self) -> Result<PathBuf>;

    /// Starts a standard (non-replay) recording session.
    async fn start_recording(&self, config: &CaptureConfig) -> Result<()>;

    /// Stops any active replay buffer or recording session cleanly.
    async fn stop(&self) -> Result<()>;

    /// Returns the current capture status.
    async fn status(&self) -> CaptureStatus;

    /// Subscribes to asynchronous capture lifecycle events.
    fn subscribe(&self) -> broadcast::Receiver<CaptureEvent>;
}

/// Abstraction over media inspection, thumbnailing, and trimming operations.
#[async_trait]
pub trait MediaProcessor: Send + Sync {
    /// Probes technical metadata for the media file at `path`.
    async fn probe(&self, path: &Path) -> Result<MediaProbeResult>;

    /// Generates a thumbnail image from `video_path` at timestamp `at_ms`.
    async fn generate_thumbnail(
        &self,
        video_path: &Path,
        at_ms: u64,
        output_path: &Path,
    ) -> Result<()>;

    /// Trims/exports a clip according to `req` and returns the output path.
    async fn trim(&self, req: &TrimRequest) -> Result<PathBuf>;
}

/// Persistence interface for the local clip library.
#[async_trait]
pub trait ClipStore: Send + Sync {
    /// Inserts or updates a clip record.
    async fn upsert_clip(&self, clip: &ClipMetadata) -> Result<()>;

    /// Fetches a clip by its [`ClipId`].
    async fn get_clip(&self, id: ClipId) -> Result<Option<ClipMetadata>>;

    /// Lists clips matching the given [`ClipFilter`].
    async fn list_clips(&self, filter: &ClipFilter) -> Result<Vec<ClipMetadata>>;

    /// Deletes a clip record by [`ClipId`], returning `true` if it existed.
    async fn delete_clip(&self, id: ClipId) -> Result<bool>;

    /// Adds a tag to an existing clip.
    async fn add_tag(&self, id: ClipId, tag: &str) -> Result<()>;

    /// Removes a tag from an existing clip.
    async fn remove_tag(&self, id: ClipId, tag: &str) -> Result<()>;
}

/// Client interface for uploading and managing shared clips on ClipForge Server.
#[async_trait]
pub trait CloudClient: Send + Sync {
    /// Verifies connectivity to the configured server.
    async fn health_check(&self) -> Result<()>;

    /// Uploads a clip video file and metadata, returning the created share info.
    async fn upload_clip(&self, clip: &ClipMetadata, file_path: &Path) -> Result<CloudShareInfo>;

    /// Retrieves metadata for an existing cloud share.
    async fn get_share(&self, share_id: &str) -> Result<CloudShareInfo>;

    /// Deletes a cloud share by ID.
    async fn delete_share(&self, share_id: &str) -> Result<()>;
}
