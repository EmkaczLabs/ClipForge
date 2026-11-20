//! Persistent `AppConfig` loading and atomic saving for ClipForge Desktop.

use std::fs;
use std::path::{Path, PathBuf};

use clipforge_common::{AppConfig, ClipForgeError, Result};

/// Returns the default path to `config.json` following XDG conventions.
pub fn config_file_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg).join("clipforge").join("config.json");
    }
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        return PathBuf::from(home)
            .join(".config")
            .join("clipforge")
            .join("config.json");
    }
    PathBuf::from("clipforge-config.json")
}

/// Loads `AppConfig` from `path`, falling back to `AppConfig::default()` if the file does not exist.
pub fn load_app_config_from(path: &Path) -> AppConfig {
    match fs::read_to_string(path) {
        Ok(raw) => match serde_json::from_str::<AppConfig>(&raw) {
            Ok(cfg) => cfg,
            Err(err) => {
                tracing::warn!(?path, %err, "Failed to parse config.json; using defaults");
                AppConfig::default()
            }
        },
        Err(_) => AppConfig::default(),
    }
}

/// Loads `AppConfig` from the default XDG config path.
pub fn load_app_config() -> AppConfig {
    load_app_config_from(&config_file_path())
}

/// Atomically writes `AppConfig` as pretty JSON to `path`.
pub fn save_app_config_to(path: &Path, config: &AppConfig) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| ClipForgeError::Serialization(e.to_string()))?;
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, json)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn config_roundtrip_temp_file() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("clipforge-cfg-test-{nanos}"));
        let path = dir.join("config.json");

        let mut cfg = AppConfig {
            server_url: Some("https://clips.example.com".to_string()),
            ..AppConfig::default()
        };
        cfg.capture.fps = 120;

        save_app_config_to(&path, &cfg).expect("save config");
        let loaded = load_app_config_from(&path);
        assert_eq!(loaded, cfg);

        let _ = fs::remove_dir_all(&dir);
    }
}
