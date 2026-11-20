use thiserror::Error;

/// Unified error type across ClipForge crates.
#[derive(Debug, Error)]
pub enum ClipForgeError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Capture backend unavailable: {0}")]
    CaptureUnavailable(String),

    #[error("Capture failed: {0}")]
    CaptureFailed(String),

    #[error("Media processing failed: {0}")]
    MediaFailed(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Cloud error: {0}")]
    Cloud(String),

    #[error("{0}")]
    Other(String),
}

/// Convenience result alias using [`ClipForgeError`].
pub type Result<T> = std::result::Result<T, ClipForgeError>;
