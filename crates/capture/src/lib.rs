//! Capture backend crate (GPU Screen Recorder integration & global hotkeys).

pub mod args;
pub mod audio;
pub mod backend;
pub mod discovery;
pub mod hotkey;

pub use args::{build_recording_args, build_replay_args, validate_config};
pub use audio::{
    AudioRoutingPlan, AudioTrackTarget, ResolvedAudioTrack, format_routed_audio_source,
    is_mic_source, is_system_source, parse_routed_audio_source, resolve_audio_routing,
};
pub use backend::GsrCaptureBackend;
pub use discovery::{
    AudioDeviceInfo, GSR_FLATPAK_ID, GSR_INSTALL_HELP, GsrCapabilities, GsrInstallation,
    MonitorInfo, detect_gsr, find_native_in_path, list_application_audio, list_audio_devices,
    list_monitors, parse_application_audio_output, parse_audio_devices_output, parse_capabilities,
    parse_monitors_output, query_capabilities, query_version,
};
pub use hotkey::{
    GlobalHotkeyStatus, HotkeyAction, HotkeyBackendKind, HotkeyManager, HotkeySpec,
    detect_hotkey_backend, parse_hotkey, select_hotkey_backend,
};

pub use clipforge_common::{CaptureBackend, CaptureConfig, CaptureEvent, CaptureStatus};
