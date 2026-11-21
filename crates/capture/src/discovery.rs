use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clipforge_common::{ClipForgeError, Result};
use tokio::process::Command;

/// Flatpak application ID for upstream GPU Screen Recorder.
pub const GSR_FLATPAK_ID: &str = "com.dec05eba.gpu_screen_recorder";

/// Actionable installation guidance returned when `gpu-screen-recorder` is missing.
pub const GSR_INSTALL_HELP: &str = "\
gpu-screen-recorder was not found in PATH or Flatpak.\n\
Install it using one of:\n\
  - Flatpak: flatpak install flathub com.dec05eba.gpu_screen_recorder\n\
  - Arch/AUR: yay -S gpu-screen-recorder\n\
  - Source: https://git.dec05eba.com/gpu-screen-recorder";

/// How `gpu-screen-recorder` is installed on the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GsrInstallation {
    /// Native binary on the host filesystem (found in `PATH` or standard locations).
    Native(PathBuf),
    /// Flatpak package invoked via `flatpak run com.dec05eba.gpu_screen_recorder`.
    Flatpak { app_id: String },
}

impl GsrInstallation {
    /// Constructs a [`tokio::process::Command`] for invoking GSR with the given arguments.
    pub fn command(&self, args: &[OsString]) -> Command {
        match self {
            GsrInstallation::Native(bin) => {
                let mut cmd = Command::new(bin);
                cmd.args(args);
                cmd
            }
            GsrInstallation::Flatpak { app_id } => {
                let mut cmd = Command::new("flatpak");
                cmd.arg("run").arg(app_id);
                cmd.args(args);
                cmd
            }
        }
    }
}

/// Discovered monitor / capture output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorInfo {
    pub name: String,
    pub resolution: Option<String>,
}

/// Discovered audio input/output device or application stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub label: String,
}

/// Summary of `gpu-screen-recorder --version` and `--info` capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GsrCapabilities {
    pub version: String,
    pub raw_info: String,
    pub supports_portal: bool,
    pub supports_nvenc: bool,
    pub supports_vaapi: bool,
    pub supports_vulkan: bool,
}

/// Searches `PATH` for an executable `gpu-screen-recorder` binary.
pub fn find_native_in_path() -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let bin_name = if cfg!(windows) {
        "gpu-screen-recorder.exe"
    } else {
        "gpu-screen-recorder"
    };
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(bin_name);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable_file(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(meta) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                meta.is_file() && (meta.permissions().mode() & 0o111 != 0)
            }
            #[cfg(not(unix))]
            {
                meta.is_file()
            }
        }
        Err(_) => false,
    }
}

/// Detects `gpu-screen-recorder` on the host (native `PATH` first, then Flatpak).
#[cfg(target_os = "linux")]
pub async fn detect_gsr() -> Result<GsrInstallation> {
    if let Some(bin) = find_native_in_path() {
        return Ok(GsrInstallation::Native(bin));
    }

    if let Ok(out) = Command::new("flatpak")
        .args(["info", GSR_FLATPAK_ID])
        .output()
        .await
        && out.status.success()
    {
        return Ok(GsrInstallation::Flatpak {
            app_id: GSR_FLATPAK_ID.to_string(),
        });
    }

    Err(ClipForgeError::CaptureUnavailable(
        GSR_INSTALL_HELP.to_string(),
    ))
}

/// Detects `gpu-screen-recorder` on non-Linux hosts (always unavailable; WGC+WASAPI unimplemented).
#[cfg(not(target_os = "linux"))]
pub async fn detect_gsr() -> Result<GsrInstallation> {
    Err(ClipForgeError::CaptureUnavailable(
        "gpu-screen-recorder is Linux-only (WGC + WASAPI capture is unimplemented on Windows)."
            .to_string(),
    ))
}

async fn run_gsr_query(install: &GsrInstallation, flag: &str) -> Result<String> {
    let args = vec![OsString::from(flag)];
    let out = install.command(&args).output().await?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(ClipForgeError::CaptureFailed(format!(
            "gpu-screen-recorder {flag} failed (exit {:?}): {stderr}",
            out.status.code()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Queries `gpu-screen-recorder --version`.
pub async fn query_version(install: &GsrInstallation) -> Result<String> {
    let stdout = run_gsr_query(install, "--version").await?;
    Ok(stdout.trim().to_string())
}

/// Queries `gpu-screen-recorder --version` and `--info`.
pub async fn query_capabilities(install: &GsrInstallation) -> Result<GsrCapabilities> {
    let version = query_version(install).await?;
    let raw_info = run_gsr_query(install, "--info").await.unwrap_or_default();
    Ok(parse_capabilities(&version, &raw_info))
}

/// Lists monitors via `gpu-screen-recorder --list-monitors`.
pub async fn list_monitors(install: &GsrInstallation) -> Result<Vec<MonitorInfo>> {
    let stdout = run_gsr_query(install, "--list-monitors").await?;
    Ok(parse_monitors_output(&stdout))
}

/// Lists audio devices via `gpu-screen-recorder --list-audio-devices`.
pub async fn list_audio_devices(install: &GsrInstallation) -> Result<Vec<AudioDeviceInfo>> {
    let stdout = run_gsr_query(install, "--list-audio-devices").await?;
    Ok(parse_audio_devices_output(&stdout))
}

/// Lists PipeWire application audio targets via `gpu-screen-recorder --list-application-audio`.
pub async fn list_application_audio(install: &GsrInstallation) -> Result<Vec<AudioDeviceInfo>> {
    let stdout = run_gsr_query(install, "--list-application-audio").await?;
    Ok(parse_application_audio_output(&stdout))
}

/// Parses `--info` output into structured capability flags.
pub fn parse_capabilities(version: &str, info: &str) -> GsrCapabilities {
    let lower = info.to_ascii_lowercase();
    GsrCapabilities {
        version: version.trim().to_string(),
        raw_info: info.to_string(),
        supports_portal: lower.contains("portal"),
        supports_nvenc: lower.contains("nvenc") || lower.contains("nvidia"),
        supports_vaapi: lower.contains("vaapi"),
        supports_vulkan: lower.contains("vulkan"),
    }
}

/// Parses `--list-monitors` output (`NAME|WIDTHxHEIGHT` or `NAME`).
pub fn parse_monitors_output(stdout: &str) -> Vec<MonitorInfo> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| match line.split_once('|') {
            Some((name, res)) => MonitorInfo {
                name: name.trim().to_string(),
                resolution: Some(res.trim().to_string()),
            },
            None => MonitorInfo {
                name: line.to_string(),
                resolution: None,
            },
        })
        .collect()
}

/// Parses `--list-audio-devices` output (`DEVICE_ID|Human Label` or `DEVICE_ID`).
pub fn parse_audio_devices_output(stdout: &str) -> Vec<AudioDeviceInfo> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| match line.split_once('|') {
            Some((id, label)) => AudioDeviceInfo {
                id: id.trim().to_string(),
                label: label.trim().to_string(),
            },
            None => AudioDeviceInfo {
                id: line.to_string(),
                label: line.to_string(),
            },
        })
        .collect()
}

/// Parses `--list-application-audio` output into `app:<name>` entries.
pub fn parse_application_audio_output(stdout: &str) -> Vec<AudioDeviceInfo> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (raw_id, label) = match line.split_once('|') {
                Some((id, label)) => (id.trim(), label.trim()),
                None => (line, line),
            };
            let id = if raw_id.starts_with("app:") {
                raw_id.to_string()
            } else {
                format!("app:{raw_id}")
            };
            AudioDeviceInfo {
                id,
                label: label.to_string(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_monitors_handles_pipe_and_plain_lines() {
        let sample = "\nDP-1|2560x1440\nHDMI-A-1|1920x1080\neDP-1\n  \n";
        let monitors = parse_monitors_output(sample);
        assert_eq!(
            monitors,
            vec![
                MonitorInfo {
                    name: "DP-1".to_string(),
                    resolution: Some("2560x1440".to_string()),
                },
                MonitorInfo {
                    name: "HDMI-A-1".to_string(),
                    resolution: Some("1920x1080".to_string()),
                },
                MonitorInfo {
                    name: "eDP-1".to_string(),
                    resolution: None,
                },
            ]
        );
    }

    #[test]
    fn parse_audio_devices_handles_pipe_and_plain() {
        let sample = "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor|Built-in Audio Analog Stereo\ndefault_input\n";
        let devices = parse_audio_devices_output(sample);
        assert_eq!(devices.len(), 2);
        assert_eq!(
            devices[0].id,
            "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor"
        );
        assert_eq!(devices[0].label, "Built-in Audio Analog Stereo");
        assert_eq!(devices[1].id, "default_input");
    }

    #[test]
    fn parse_application_audio_prefixes_app() {
        let sample = "discord\napp:obs|OBS Studio\n";
        let apps = parse_application_audio_output(sample);
        assert_eq!(
            apps,
            vec![
                AudioDeviceInfo {
                    id: "app:discord".to_string(),
                    label: "discord".to_string(),
                },
                AudioDeviceInfo {
                    id: "app:obs".to_string(),
                    label: "OBS Studio".to_string(),
                },
            ]
        );
    }

    #[test]
    fn parse_capabilities_detects_backends() {
        let caps = parse_capabilities(
            "4.2.0",
            "Supported codecs: h264, hevc\nVAAPI: yes\nVulkan: yes\nPortal: yes\n",
        );
        assert_eq!(caps.version, "4.2.0");
        assert!(caps.supports_vaapi);
        assert!(caps.supports_vulkan);
        assert!(caps.supports_portal);
        assert!(!caps.supports_nvenc);
    }
}
