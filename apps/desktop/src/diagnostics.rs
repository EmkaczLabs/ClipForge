//! Sanitized diagnostic report generation and export for ClipForge Desktop.
//!
//! Never includes API tokens, passwords, S3 keys, or private clip contents.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use clipforge_common::{ClipForgeError, Result};

use crate::worker::DependencyStatus;

/// Sanitized diagnostic snapshot safe to share in bug reports.
#[derive(Debug, Clone)]
pub struct DiagnosticsReport {
    pub app_version: String,
    pub os: String,
    pub arch: String,
    pub display_session: String,
    pub config_path: String,
    pub clips_dir: String,
    pub capture_backend: String,
    pub capture_status: String,
    pub hotkey_status: String,
    pub dependencies: DependencyStatus,
    pub audio_routing_summary: String,
    pub server_url_redacted: Option<String>,
    pub api_token_configured: bool,
    pub generated_at: String,
}

impl DiagnosticsReport {
    /// Formats the report as pretty-printed JSON.
    pub fn to_pretty_json(&self) -> Result<String> {
        let value = serde_json::json!({
            "app_version": self.app_version,
            "os": self.os,
            "arch": self.arch,
            "display_session": self.display_session,
            "generated_at": self.generated_at,
            "paths": {
                "config_path": self.config_path,
                "clips_dir": self.clips_dir,
            },
            "capture": {
                "backend": self.capture_backend,
                "status": self.capture_status,
                "hotkey_status": self.hotkey_status,
            },
            "dependencies": {
                "probed": self.dependencies.probed,
                "gsr_available": self.dependencies.gsr_available,
                "gsr_location": self.dependencies.gsr_location,
                "ffmpeg_available": self.dependencies.ffmpeg_available,
                "ffprobe_available": self.dependencies.ffprobe_available,
            },
            "audio_routing_summary": self.audio_routing_summary,
            "cloud": {
                "server_url": self.server_url_redacted,
                "api_token_configured": self.api_token_configured,
            }
        });
        serde_json::to_string_pretty(&value)
            .map_err(|e| ClipForgeError::Serialization(e.to_string()))
    }
}

/// Redacts credentials, query strings, and fragments from a server URL.
pub fn redact_server_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let without_frag = trimmed.split('#').next().unwrap_or(trimmed);
    let without_query = without_frag.split('?').next().unwrap_or(without_frag);
    if let Some((scheme, rest)) = without_query.split_once("://") {
        let authority_and_path = match rest.split_once('@') {
            Some((_userinfo, after_at)) => format!("[redacted]@{after_at}"),
            None => rest.to_string(),
        };
        format!("{scheme}://{authority_and_path}")
    } else {
        without_query.to_string()
    }
}

/// Atomically writes a sanitized diagnostic JSON report into `dir` and returns its path.
pub fn export_diagnostics_to_dir(dir: &Path, report: &DiagnosticsReport) -> Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    let target = dir.join(format!("clipforge-diagnostics-{stamp}.json"));
    let tmp = dir.join(format!("clipforge-diagnostics-{stamp}.json.tmp"));
    let json = report.to_pretty_json()?;
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_server_url_strips_credentials_query_and_fragment() {
        assert_eq!(
            redact_server_url("https://user:s3cr3t@clips.emkacz.dev:8443/api?token=cf_pat_123#top"),
            "https://[redacted]@clips.emkacz.dev:8443/api"
        );
        assert_eq!(
            redact_server_url("https://clips.emkacz.dev/"),
            "https://clips.emkacz.dev/"
        );
        assert_eq!(redact_server_url("   "), "");
    }

    #[test]
    fn export_diagnostics_writes_sanitized_json_without_secrets() {
        let dir = std::env::temp_dir().join(format!(
            "clipforge-diag-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));

        let report = DiagnosticsReport {
            app_version: "0.1.0".to_string(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            display_session: if cfg!(target_os = "windows") {
                "windows".to_string()
            } else {
                "x11".to_string()
            },
            config_path: "/tmp/config.json".to_string(),
            clips_dir: "/tmp/clips".to_string(),
            capture_backend: if cfg!(target_os = "linux") {
                "gpu-screen-recorder".to_string()
            } else {
                "unimplemented (WGC+WASAPI pending)".to_string()
            },
            capture_status: "Idle".to_string(),
            hotkey_status: "Focused-Window Only".to_string(),
            dependencies: DependencyStatus {
                gsr_available: false,
                gsr_location: None,
                ffmpeg_available: false,
                ffprobe_available: false,
                probed: true,
            },
            audio_routing_summary: "Auto (2 tracks)".to_string(),
            server_url_redacted: Some(redact_server_url(
                "https://alice:hunter2@clips.emkacz.dev?api_token=cf_pat_secret",
            )),
            api_token_configured: true,
            generated_at: "2026-11-21T22:00:00Z".to_string(),
        };

        let out_path = export_diagnostics_to_dir(&dir, &report).expect("export diagnostics");
        assert!(out_path.is_file());
        let contents = fs::read_to_string(&out_path).expect("read exported diagnostics");
        assert!(contents.contains("\"app_version\": \"0.1.0\""));
        assert!(contents.contains("\"api_token_configured\": true"));
        assert!(!contents.contains("hunter2"));
        assert!(!contents.contains("cf_pat_secret"));

        let _ = fs::remove_dir_all(&dir);
    }
}
