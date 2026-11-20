//! Shared domain types, error definitions, logging setup, and trait contracts for ClipForge.

pub mod error;
pub mod logging;
pub mod models;
pub mod traits;

pub use error::{ClipForgeError, Result};
pub use logging::init_tracing;
pub use models::*;
pub use traits::{CaptureBackend, ClipStore, CloudClient, MediaProcessor};
