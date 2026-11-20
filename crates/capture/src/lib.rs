//! Capture backend crate (GPU Screen Recorder integration & X11 global hotkeys).

pub mod args;
pub mod backend;
pub mod discovery;
pub mod hotkey;

pub use args::{build_recording_args, build_replay_args, validate_config};
pub use backend::GsrCaptureBackend;
pub use discovery::{
    AudioDeviceInfo, GSR_FLATPAK_ID, GSR_INSTALL_HELP, GsrCapabilities, GsrInstallation,
    MonitorInfo, detect_gsr, find_native_in_path, list_application_audio, list_audio_devices,
    list_monitors, parse_application_audio_output, parse_audio_devices_output, parse_capabilities,
    parse_monitors_output, query_capabilities, query_version,
};
pub use hotkey::{HotkeyAction, HotkeyManager, HotkeySpec, parse_hotkey};

pub use clipforge_common::{CaptureBackend, CaptureConfig, CaptureEvent, CaptureStatus};
