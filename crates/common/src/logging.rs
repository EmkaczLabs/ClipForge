use tracing_subscriber::{EnvFilter, fmt};

/// Initializes structured tracing/logging for ClipForge binaries.
///
/// Safe to call multiple times; subsequent calls are ignored if a global
/// subscriber is already registered.
pub fn init_tracing(default_level: &str) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    let _ = fmt().with_env_filter(filter).with_target(true).try_init();
}
