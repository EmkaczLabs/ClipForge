//! Media processing crate (`ffmpeg` / `ffprobe` wrapper).

pub mod processor;

pub use clipforge_common::{MediaProbeResult, MediaProcessor, TrimRequest};
pub use processor::FfmpegMediaProcessor;
