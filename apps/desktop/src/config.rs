//! Persistent `AppConfig` loading, atomic saving, and secure API token storage for ClipForge Desktop.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use clipforge_common::{AppConfig, ClipForgeError, Result};

const SECRET_SERVICE: &str = "clipforge";

fn config_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg).join("clipforge");
    }
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        return PathBuf::from(home).join(".config").join("clipforge");
    }
    PathBuf::from(".")
}

/// Returns the default path to `config.json` following XDG conventions.
pub fn config_file_path() -> PathBuf {
    let dir = config_dir();
    if dir == Path::new(".") {
        PathBuf::from("clipforge-config.json")
    } else {
        dir.join("config.json")
    }
}

/// Returns the fallback `0600` user-only token file path (`credentials.json`).
pub fn token_file_path() -> PathBuf {
    let dir = config_dir();
    if dir == Path::new(".") {
        PathBuf::from("clipforge-credentials.json")
    } else {
        dir.join("credentials.json")
    }
}

fn normalize_server_key(server_url: &str) -> String {
    server_url.trim().trim_end_matches('/').to_string()
}

/// Loads `AppConfig` from `path`, falling back to `AppConfig::default()` if the file does not exist.
///
/// If a legacy `config.json` contains a plaintext `api_token`, it is migrated to secure credential
/// storage and stripped from `config.json`.
pub fn load_app_config_from(path: &Path) -> AppConfig {
    match fs::read_to_string(path) {
        Ok(raw) => match serde_json::from_str::<AppConfig>(&raw) {
            Ok(mut cfg) => {
                if let Some(legacy_token) = cfg.api_token.take()
                    && !legacy_token.trim().is_empty()
                {
                    if let Some(url) = cfg.server_url.as_deref().filter(|u| !u.trim().is_empty()) {
                        let _ = store_api_token(url, legacy_token.trim());
                    }
                    let _ = save_app_config_to(path, &cfg);
                }
                cfg
            }
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

/// Atomically writes `AppConfig` as pretty JSON to `path`, redacting `api_token` so secrets are
/// never persisted in plaintext `config.json`.
pub fn save_app_config_to(path: &Path, config: &AppConfig) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let mut sanitized = config.clone();
    sanitized.api_token = None;
    let json = serde_json::to_string_pretty(&sanitized)
        .map_err(|e| ClipForgeError::Serialization(e.to_string()))?;
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, json)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Stores an API token for `server_url` using OS secret storage (`secret-tool` / Secret Service
/// D-Bus) when available, falling back to a `0600` user-only token file.
pub fn store_api_token(server_url: &str, token: &str) -> Result<()> {
    let key = normalize_server_key(server_url);
    let clean_token = token.trim();
    if key.is_empty() || clean_token.is_empty() {
        return Ok(());
    }

    if try_secret_tool_store(&key, clean_token) {
        let _ = delete_api_token_from_file(&token_file_path(), &key);
        return Ok(());
    }

    store_api_token_in_file(&token_file_path(), &key, clean_token)
}

/// Loads the stored API token for `server_url` from OS secret storage or the `0600` fallback file.
pub fn load_api_token(server_url: &str) -> Option<String> {
    let key = normalize_server_key(server_url);
    if key.is_empty() {
        return None;
    }

    if let Some(tok) = try_secret_tool_lookup(&key) {
        return Some(tok);
    }

    load_api_token_from_file(&token_file_path(), &key)
}

/// Deletes the stored API token for `server_url` from both OS secret storage and the fallback file.
pub fn delete_api_token(server_url: &str) -> Result<()> {
    let key = normalize_server_key(server_url);
    if key.is_empty() {
        return Ok(());
    }

    let _ = try_secret_tool_clear(&key);
    delete_api_token_from_file(&token_file_path(), &key)
}

fn try_secret_tool_store(server_key: &str, token: &str) -> bool {
    let Ok(mut child) = Command::new("secret-tool")
        .args([
            "store",
            "--label=ClipForge Cloud Token",
            "service",
            SECRET_SERVICE,
            "server",
            server_key,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(token.as_bytes());
    }
    child.wait().is_ok_and(|s| s.success())
}

fn try_secret_tool_lookup(server_key: &str) -> Option<String> {
    let output = Command::new("secret-tool")
        .args(["lookup", "service", SECRET_SERVICE, "server", server_key])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if val.is_empty() { None } else { Some(val) }
}

fn try_secret_tool_clear(server_key: &str) -> bool {
    Command::new("secret-tool")
        .args(["clear", "service", SECRET_SERVICE, "server", server_key])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn read_token_map(path: &Path) -> BTreeMap<String, String> {
    let Ok(raw) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn write_token_map(path: &Path, map: &BTreeMap<String, String>) -> Result<()> {
    if map.is_empty() {
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        return Ok(());
    }

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }

    let json = serde_json::to_string_pretty(map)
        .map_err(|e| ClipForgeError::Serialization(e.to_string()))?;
    let tmp_path = path.with_extension("json.tmp");

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp_path)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        fs::write(&tmp_path, json)?;
    }

    fs::rename(&tmp_path, path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

fn store_api_token_in_file(path: &Path, server_key: &str, token: &str) -> Result<()> {
    let mut map = read_token_map(path);
    map.insert(server_key.to_string(), token.to_string());
    write_token_map(path, &map)
}

fn load_api_token_from_file(path: &Path, server_key: &str) -> Option<String> {
    let map = read_token_map(path);
    map.get(server_key).cloned().filter(|t| !t.is_empty())
}

fn delete_api_token_from_file(path: &Path, server_key: &str) -> Result<()> {
    let mut map = read_token_map(path);
    if map.remove(server_key).is_some() {
        write_token_map(path, &map)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("{prefix}-{nanos}"))
    }

    #[test]
    fn config_roundtrip_temp_file_redacts_api_token() {
        let dir = unique_temp_dir("clipforge-cfg-test");
        let path = dir.join("config.json");

        let mut cfg = AppConfig {
            server_url: Some("https://clips.example.com".to_string()),
            api_token: Some("cf_secret_should_never_persist".to_string()),
            ..AppConfig::default()
        };
        cfg.capture.fps = 120;

        save_app_config_to(&path, &cfg).expect("save config");
        let raw = fs::read_to_string(&path).expect("read config");
        assert!(
            !raw.contains("cf_secret_should_never_persist"),
            "api_token must never be persisted in plaintext config.json"
        );

        let loaded = load_app_config_from(&path);
        assert_eq!(loaded.server_url, cfg.server_url);
        assert_eq!(loaded.capture.fps, 120);
        assert!(loaded.api_token.is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fallback_token_file_uses_0600_permissions_and_roundtrips() {
        let dir = unique_temp_dir("clipforge-tok-test");
        let path = dir.join("credentials.json");
        let key = "https://clips.example.com";

        store_api_token_in_file(&path, key, "cf_tok_12345").expect("store token");
        assert_eq!(
            load_api_token_from_file(&path, key).as_deref(),
            Some("cf_tok_12345")
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "token file must be 0600 user-only");
        }

        delete_api_token_from_file(&path, key).expect("delete token");
        assert!(load_api_token_from_file(&path, key).is_none());
        assert!(!path.exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
