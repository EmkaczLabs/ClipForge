//! Media processing and off-UI-thread video decoding (`ffmpeg` / `ffprobe` wrapper).

pub mod audio;
pub mod mic_test;
pub mod player;
pub mod processor;

pub use clipforge_common::{MediaProbeResult, MediaProcessor, TrimRequest};
pub use mic_test::{
    DBFS_FLOOR, DEFAULT_MIC_TEST_DURATION, MicDiagnosticSummary, MicFeedbackKind, MicTestEvent,
    MicTestHandle, MicTestPhase, classify_mic_signal, compute_dbfs,
};
pub use player::{
    DEFAULT_MAX_DECODE_DIM, DecodedFrame, PlaybackState, PlayerEvent, VideoMetadata,
    VideoPlayerHandle, extract_timeline_thumbnails, extract_timeline_thumbnails_with_binaries,
};
pub use processor::{ExportCancelToken, FfmpegMediaProcessor};
