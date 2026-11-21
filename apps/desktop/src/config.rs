//! Persistent `AppConfig` loading, atomic saving, and secure API token storage for ClipForge Desktop.

use std::collections::BTreeMap;
use std::fs;
#[cfg(unix)]
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};

use clipforge_common::{AppConfig, ClipForgeError, Result};

#[cfg(target_os = "linux")]
const SECRET_SERVICE: &str = "clipforge";

fn config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA")
            && !appdata.is_empty()
        {
            return PathBuf::from(appdata).join("clipforge");
        }
        if let Ok(profile) = std::env::var("USERPROFILE")
            && !profile.is_empty()
        {
            return PathBuf::from(profile).join(".config").join("clipforge");
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
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
    }
    PathBuf::from(".")
}

/// Returns the directory used for sanitized diagnostic exports.
pub fn diagnostics_dir() -> PathBuf {
    config_dir().join("diagnostics")
}

/// Returns the default path to `config.json` following OS config conventions.
pub fn config_file_path() -> PathBuf {
    let dir = config_dir();
    if dir == Path::new(".") {
        PathBuf::from("clipforge-config.json")
    } else {
        dir.join("config.json")
    }
}

/// Returns the legacy fallback token file path (`credentials.json`) for cleanup purposes.
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
                    if let Some(url) = cfg.server_url.as_deref().filter(|u| !u.trim().is_empty())
                        && let Err(err) = store_api_token(url, legacy_token.trim())
                    {
                        tracing::warn!(
                            server = %url,
                            %err,
                            "Failed to migrate legacy api_token to OS secret storage; stripping plaintext token from config.json"
                        );
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
/// D-Bus). Fails explicitly rather than silently falling back to plaintext disk storage.
pub fn store_api_token(server_url: &str, token: &str) -> Result<()> {
    let key = normalize_server_key(server_url);
    let clean_token = token.trim();
    if key.is_empty() || clean_token.is_empty() {
        return Ok(());
    }

    match try_secret_tool_store(&key, clean_token) {
        Ok(()) => {
            let _ = delete_api_token_from_file(&token_file_path(), &key);
            Ok(())
        }
        Err(reason) => {
            tracing::warn!(
                server = %key,
                %reason,
                "OS secret storage unavailable; refusing to persist API token in plaintext"
            );
            Err(ClipForgeError::Other(format!(
                "OS secret storage (secret-tool / org.freedesktop.secrets) unavailable: {reason}. Token was not saved to disk."
            )))
        }
    }
}

/// Loads the stored API token for `server_url` from OS secret storage.
pub fn load_api_token(server_url: &str) -> Option<String> {
    let key = normalize_server_key(server_url);
    if key.is_empty() {
        return None;
    }

    if let Some(tok) = try_secret_tool_lookup(&key) {
        return Some(tok);
    }

    let legacy_path = token_file_path();
    if read_token_map(&legacy_path).contains_key(&key) {
        tracing::warn!(
            server = %key,
            path = ?legacy_path,
            "Ignoring legacy plaintext credentials.json entry; please sign in again to store token in OS secret storage"
        );
    }
    None
}

/// Deletes the stored API token for `server_url` from OS secret storage and cleans up any legacy
/// `credentials.json` entry if present.
pub fn delete_api_token(server_url: &str) -> Result<()> {
    let key = normalize_server_key(server_url);
    if key.is_empty() {
        return Ok(());
    }

    let _ = try_secret_tool_clear(&key);
    delete_api_token_from_file(&token_file_path(), &key)
}

#[cfg(target_os = "linux")]
fn try_secret_tool_store(server_key: &str, token: &str) -> std::result::Result<(), String> {
    let mut child = Command::new("secret-tool")
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
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn secret-tool: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(token.as_bytes())
            .map_err(|e| format!("failed to write token to secret-tool stdin: {e}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("failed waiting for secret-tool: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(format!("secret-tool exited with {}", output.status))
        } else {
            Err(format!("secret-tool failed: {stderr}"))
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn try_secret_tool_store(_server_key: &str, _token: &str) -> std::result::Result<(), String> {
    Err("Windows Credential Manager integration is not yet implemented".to_string())
}

#[cfg(target_os = "linux")]
fn try_secret_tool_lookup(server_key: &str) -> Option<String> {
    let output = Command::new("secret-tool")
        .args(["lookup", "service", SECRET_SERVICE, "server", server_key])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .ok()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !stderr.is_empty() {
            tracing::warn!(server = %server_key, %stderr, "secret-tool lookup failed");
        }
        return None;
    }
    let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if val.is_empty() { None } else { Some(val) }
}

#[cfg(not(target_os = "linux"))]
fn try_secret_tool_lookup(_server_key: &str) -> Option<String> {
    None
}

#[cfg(target_os = "linux")]
fn try_secret_tool_clear(server_key: &str) -> bool {
    let Ok(output) = Command::new("secret-tool")
        .args(["clear", "service", SECRET_SERVICE, "server", server_key])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !stderr.is_empty() {
            tracing::debug!(server = %server_key, %stderr, "secret-tool clear failed");
        }
    }
    output.status.success()
}

#[cfg(not(target_os = "linux"))]
fn try_secret_tool_clear(_server_key: &str) -> bool {
    false
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
    fn legacy_token_file_cleanup_removes_entry_and_empty_file() {
        let dir = unique_temp_dir("clipforge-tok-test");
        let path = dir.join("credentials.json");
        let key = "https://clips.example.com";

        let mut map = BTreeMap::new();
        map.insert(key.to_string(), "cf_tok_12345".to_string());
        write_token_map(&path, &map).expect("write legacy token");
        assert!(path.exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "legacy token file must be 0600 user-only");
        }

        delete_api_token_from_file(&path, key).expect("delete token");
        assert!(!path.exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
