//! Media processing and off-UI-thread video decoding (`ffmpeg` / `ffprobe` wrapper).

pub mod audio;
pub mod player;
pub mod processor;

pub use clipforge_common::{MediaProbeResult, MediaProcessor, TrimRequest};
pub use player::{
    DEFAULT_MAX_DECODE_DIM, DecodedFrame, PlaybackState, PlayerEvent, VideoMetadata,
    VideoPlayerHandle, extract_timeline_thumbnails, extract_timeline_thumbnails_with_binaries,
};
pub use processor::{ExportCancelToken, FfmpegMediaProcessor};
