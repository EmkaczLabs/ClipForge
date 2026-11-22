//! Capture backend crate (GPU Screen Recorder on Linux, WGC + WASAPI + FFmpeg on Windows, & global hotkeys).

pub mod args;
pub mod audio;
pub mod backend;
pub mod discovery;
pub mod hotkey;
#[cfg(target_os = "windows")]
pub mod windows_audio;
pub mod windows_backend;
pub mod windows_encoder;
pub mod windows_replay;
#[cfg(target_os = "windows")]
pub mod windows_wgc;

pub use args::{build_recording_args, build_replay_args, validate_config};
pub use audio::{
    AudioRoutingPlan, AudioTrackTarget, ResolvedAudioTrack, format_routed_audio_source,
    is_mic_source, is_system_source, parse_routed_audio_source, resolve_audio_routing,
};
pub use backend::{GsrCaptureBackend, create_capture_backend};
pub use discovery::{
    AudioDeviceInfo, CaptureCapabilityProbe, GSR_FLATPAK_ID, GSR_INSTALL_HELP, GsrCapabilities,
    GsrInstallation, MonitorInfo, detect_gsr, discover_application_audio, discover_audio_devices,
    discover_monitors, find_native_in_path, list_application_audio, list_audio_devices,
    list_monitors, parse_application_audio_output, parse_audio_devices_output, parse_capabilities,
    parse_monitors_output, probe_capture_capabilities, query_capabilities, query_version,
};
pub use hotkey::{
    GlobalHotkeyStatus, HotkeyAction, HotkeyBackendKind, HotkeyManager, HotkeySpec,
    WindowsHotkeyCode, detect_hotkey_backend, hotkey_spec_to_windows_code, parse_hotkey,
    parse_windows_hotkey, select_hotkey_backend,
};
pub use windows_backend::{
    BorderlessAccessState, WindowsAudioSelection, WindowsCaptureBackend,
    probe_borderless_capture_status, request_borderless_access,
    request_borderless_access_for_config, select_windows_audio_devices,
};
pub use windows_encoder::{
    CREATE_NO_WINDOW, WindowsEncoderParams, WindowsOutputMode, build_windows_ffmpeg_args,
    find_ffmpeg_binary, mix_audio_chunks, normalize_channels_interleaved,
    parse_ffmpeg_encoders_output, probe_ffmpeg_encoders, resample_linear_interleaved,
    select_windows_video_encoder,
};
pub use windows_replay::{
    REPLAY_SEGMENT_TIME_SECS, build_concat_ffmpeg_args, build_concat_manifest,
    compute_replay_ring_slots, container_extension, order_replay_segments,
};

pub use clipforge_common::{
    BorderlessCaptureStatus, CaptureBackend, CaptureConfig, CaptureEvent, CaptureStatus,
};
