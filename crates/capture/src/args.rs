use std::ffi::OsString;
use std::path::Path;

use clipforge_common::{
    AudioCodec, CaptureConfig, CaptureTarget, ClipForgeError, ContainerFormat, EncoderMode,
    QualityPreset, Result, VideoCodec,
};

/// Minimum replay buffer duration supported by `gpu-screen-recorder` (seconds).
pub const MIN_REPLAY_SECS: u32 = 2;
/// Maximum replay buffer duration supported by `gpu-screen-recorder` (seconds).
pub const MAX_REPLAY_SECS: u32 = 86_400;

/// Validates `CaptureConfig` fields common to both replay and regular recording modes.
pub fn validate_config(config: &CaptureConfig) -> Result<()> {
    if config.fps == 0 || config.fps > 480 {
        return Err(ClipForgeError::InvalidInput(format!(
            "fps must be between 1 and 480, got {}",
            config.fps
        )));
    }

    if config.output_dir.as_os_str().is_empty() {
        return Err(ClipForgeError::InvalidInput(
            "output_dir must not be empty".to_string(),
        ));
    }

    match (config.width, config.height) {
        (Some(0), _) | (_, Some(0)) => {
            return Err(ClipForgeError::InvalidInput(
                "capture width and height must be non-zero".to_string(),
            ));
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err(ClipForgeError::InvalidInput(
                "capture width and height must both be set or both omitted".to_string(),
            ));
        }
        _ => {}
    }

    if let CaptureTarget::Monitor(ref name) = config.capture_target {
        validate_safe_token(name, "monitor name")?;
    }

    if let CaptureTarget::Region { width, height, .. } = config.capture_target
        && (width == 0 || height == 0)
    {
        return Err(ClipForgeError::InvalidInput(
            "region width and height must be non-zero".to_string(),
        ));
    }

    for src in &config.audio_sources {
        validate_safe_token(src, "audio source")?;
    }

    Ok(())
}

fn validate_safe_token(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(ClipForgeError::InvalidInput(format!(
            "{label} must not be empty"
        )));
    }
    if value.contains('\0') || value.contains('\n') || value.contains('\r') {
        return Err(ClipForgeError::InvalidInput(format!(
            "{label} contains invalid control characters"
        )));
    }
    Ok(())
}

fn push_pair(args: &mut Vec<OsString>, flag: &str, value: impl Into<OsString>) {
    args.push(OsString::from(flag));
    args.push(value.into());
}

fn append_common_args(args: &mut Vec<OsString>, config: &CaptureConfig) {
    match &config.capture_target {
        CaptureTarget::PrimaryScreen => push_pair(args, "-w", "screen"),
        CaptureTarget::Monitor(name) => push_pair(args, "-w", name),
        CaptureTarget::FocusedWindow => {
            push_pair(args, "-w", "focused");
            let (w, h) = match (config.width, config.height) {
                (Some(w), Some(h)) => (w, h),
                _ => (1920, 1080),
            };
            push_pair(args, "-s", format!("{w}x{h}"));
        }
        CaptureTarget::Portal => {
            push_pair(args, "-w", "portal");
            push_pair(
                args,
                "-restore-portal-session",
                if config.restore_portal_session {
                    "yes"
                } else {
                    "no"
                },
            );
        }
        CaptureTarget::Region {
            width,
            height,
            x,
            y,
        } => {
            push_pair(args, "-w", "region");
            push_pair(args, "-region", format!("{width}x{height}+{x}+{y}"));
        }
    }

    if !matches!(config.capture_target, CaptureTarget::FocusedWindow)
        && let (Some(w), Some(h)) = (config.width, config.height)
    {
        push_pair(args, "-s", format!("{w}x{h}"));
    }

    push_pair(args, "-c", container_flag(config.container));
    push_pair(args, "-k", video_codec_flag(config.video_codec));
    push_pair(args, "-ac", audio_codec_flag(config.audio_codec));
    push_pair(args, "-q", quality_flag(config.quality));
    push_pair(args, "-encoder", encoder_flag(config.encoder));
    push_pair(args, "-f", config.fps.to_string());

    for src in &config.audio_sources {
        push_pair(args, "-a", src);
    }
}

/// Builds validated CLI arguments for `gpu-screen-recorder` in replay buffer (`-r`) mode.
pub fn build_replay_args(
    config: &CaptureConfig,
    save_script_path: Option<&Path>,
    ipc_socket: Option<&Path>,
) -> Result<Vec<OsString>> {
    validate_config(config)?;

    if !(MIN_REPLAY_SECS..=MAX_REPLAY_SECS).contains(&config.replay_buffer_secs) {
        return Err(ClipForgeError::InvalidInput(format!(
            "replay_buffer_secs must be between {MIN_REPLAY_SECS} and {MAX_REPLAY_SECS}, got {}",
            config.replay_buffer_secs
        )));
    }

    let mut args = Vec::new();
    append_common_args(&mut args, config);

    push_pair(&mut args, "-r", config.replay_buffer_secs.to_string());
    push_pair(
        &mut args,
        "-restart-replay-on-save",
        if config.restart_replay_on_save {
            "yes"
        } else {
            "no"
        },
    );
    push_pair(&mut args, "-bm", "auto");

    if let Some(script) = save_script_path {
        push_pair(&mut args, "-sc", script.as_os_str());
    }
    if let Some(socket) = ipc_socket {
        push_pair(&mut args, "-ipc", socket.as_os_str());
    }

    push_pair(&mut args, "-o", config.output_dir.as_os_str());
    Ok(args)
}

/// Builds validated CLI arguments for `gpu-screen-recorder` in standard recording mode.
pub fn build_recording_args(config: &CaptureConfig, output_file: &Path) -> Result<Vec<OsString>> {
    validate_config(config)?;

    if output_file.as_os_str().is_empty() {
        return Err(ClipForgeError::InvalidInput(
            "output_file must not be empty".to_string(),
        ));
    }

    let mut args = Vec::new();
    append_common_args(&mut args, config);
    push_pair(&mut args, "-o", output_file.as_os_str());
    Ok(args)
}

pub fn container_flag(c: ContainerFormat) -> &'static str {
    match c {
        ContainerFormat::Mp4 => "mp4",
        ContainerFormat::Mkv => "mkv",
        ContainerFormat::Webm => "webm",
        ContainerFormat::Flv => "flv",
    }
}

pub fn video_codec_flag(v: VideoCodec) -> &'static str {
    match v {
        VideoCodec::Auto => "auto",
        VideoCodec::H264 => "h264",
        VideoCodec::Hevc => "hevc",
        VideoCodec::Av1 => "av1",
        VideoCodec::Vp8 => "vp8",
        VideoCodec::Vp9 => "vp9",
    }
}

pub fn audio_codec_flag(a: AudioCodec) -> &'static str {
    match a {
        AudioCodec::Opus => "opus",
        AudioCodec::Aac => "aac",
        AudioCodec::Flac => "flac",
    }
}

pub fn quality_flag(q: QualityPreset) -> &'static str {
    match q {
        QualityPreset::Medium => "medium",
        QualityPreset::High => "high",
        QualityPreset::VeryHigh => "very_high",
        QualityPreset::Ultra => "ultra",
    }
}

pub fn encoder_flag(e: EncoderMode) -> &'static str {
    match e {
        EncoderMode::Gpu => "gpu",
        EncoderMode::Cpu => "cpu",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn to_strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn replay_args_defaults() {
        let cfg = CaptureConfig::default();
        let args = to_strings(&build_replay_args(&cfg, None, None).expect("valid config"));
        assert!(args.windows(2).any(|w| w == ["-w", "screen"]));
        assert!(args.windows(2).any(|w| w == ["-c", "mp4"]));
        assert!(args.windows(2).any(|w| w == ["-k", "auto"]));
        assert!(args.windows(2).any(|w| w == ["-ac", "opus"]));
        assert!(args.windows(2).any(|w| w == ["-q", "high"]));
        assert!(args.windows(2).any(|w| w == ["-encoder", "gpu"]));
        assert!(args.windows(2).any(|w| w == ["-f", "60"]));
        assert!(args.windows(2).any(|w| w == ["-r", "60"]));
        assert!(args.windows(2).any(|w| w == ["-bm", "auto"]));
        assert!(args.windows(2).any(|w| w == ["-a", "default_output"]));
        assert!(args.windows(2).any(|w| w == ["-o", "clips"]));
    }

    #[test]
    fn quality_flag_matches_gsr_cli_values() {
        assert_eq!(quality_flag(QualityPreset::Medium), "medium");
        assert_eq!(quality_flag(QualityPreset::High), "high");
        assert_eq!(quality_flag(QualityPreset::VeryHigh), "very_high");
        assert_eq!(quality_flag(QualityPreset::Ultra), "ultra");
    }

    #[test]
    fn replay_args_portal_and_multiple_audio() {
        let cfg = CaptureConfig {
            capture_target: CaptureTarget::Portal,
            restore_portal_session: true,
            restart_replay_on_save: true,
            audio_sources: vec!["default_output".to_string(), "default_input".to_string()],
            ..CaptureConfig::default()
        };
        let script = PathBuf::from("/tmp/on_save.sh");
        let args = to_strings(&build_replay_args(&cfg, Some(&script), None).expect("valid"));
        assert!(args.windows(2).any(|w| w == ["-w", "portal"]));
        assert!(
            args.windows(2)
                .any(|w| w == ["-restore-portal-session", "yes"])
        );
        assert!(
            args.windows(2)
                .any(|w| w == ["-restart-replay-on-save", "yes"])
        );
        assert!(args.windows(2).any(|w| w == ["-sc", "/tmp/on_save.sh"]));
        assert!(args.windows(2).any(|w| w == ["-a", "default_output"]));
        assert!(args.windows(2).any(|w| w == ["-a", "default_input"]));
    }

    #[test]
    fn replay_args_focused_window_sets_size() {
        let cfg = CaptureConfig {
            capture_target: CaptureTarget::FocusedWindow,
            width: Some(1280),
            height: Some(720),
            ..CaptureConfig::default()
        };
        let args = to_strings(&build_replay_args(&cfg, None, None).expect("valid"));
        assert!(args.windows(2).any(|w| w == ["-w", "focused"]));
        assert!(args.windows(2).any(|w| w == ["-s", "1280x720"]));
    }

    #[test]
    fn replay_args_region_sets_geometry() {
        let cfg = CaptureConfig {
            capture_target: CaptureTarget::Region {
                width: 800,
                height: 600,
                x: 100,
                y: 50,
            },
            ..CaptureConfig::default()
        };
        let args = to_strings(&build_replay_args(&cfg, None, None).expect("valid"));
        assert!(args.windows(2).any(|w| w == ["-w", "region"]));
        assert!(args.windows(2).any(|w| w == ["-region", "800x600+100+50"]));
    }

    #[test]
    fn invalid_replay_duration_rejected() {
        let cfg = CaptureConfig {
            replay_buffer_secs: 1,
            ..CaptureConfig::default()
        };
        assert!(build_replay_args(&cfg, None, None).is_err());

        let cfg_too_long = CaptureConfig {
            replay_buffer_secs: 100_000,
            ..CaptureConfig::default()
        };
        assert!(build_replay_args(&cfg_too_long, None, None).is_err());
    }

    #[test]
    fn invalid_fps_and_audio_rejected() {
        let cfg = CaptureConfig {
            fps: 0,
            ..CaptureConfig::default()
        };
        assert!(build_recording_args(&cfg, Path::new("/tmp/out.mp4")).is_err());

        let cfg_bad_audio = CaptureConfig {
            audio_sources: vec!["bad\nsource".to_string()],
            ..CaptureConfig::default()
        };
        assert!(build_recording_args(&cfg_bad_audio, Path::new("/tmp/out.mp4")).is_err());
    }
}
