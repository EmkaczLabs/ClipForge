# ClipForge Progress Checkpoint

**Checkpoint Timestamp:** 2026-11-22T03:26:00+01:00  
**Latest Verified Commit (`ClipForge`):** `aae63b7` (`main`)  
**Latest Verified Commit (`ClipForge-Server`):** `9216eb2` (`main` at `/home/emkacz/Projects/ClipForge-Server`, deployed on Proxmox LXC `112`)  
**Host Environment:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.

## 1. Current Project Architecture & Implementation Status

| Component | Status | Notes |
|---|---|---|
| Desktop Foundation & Obsidian Forge UI (`ClipForge/apps/desktop`) | Verified & passing (`a980e92`, `aae63b7`) | 5-screen `NavSection` (`Library` default, `Editor`, `Recording`, `Uploads`, `Settings`), 7-tier Obsidian Forge palette, SVG icon & primitive library, 48px topbar + 64px rail + 28px status bar, `windows_subsystem = "windows"` + persistent file logging + `CREATE_NO_WINDOW` (`0x0800_0000`) on all subprocesses. |
| Linux Capture, Audio Routing & Hotkeys (`ClipForge/crates/capture`) | Verified (live GSR + `ffprobe`) | GSR 6.1.7 supervision (`-bm auto`, `very_high`, `CLIPFORGE_SAVED:` deduplication), deterministic `Auto`/`T1`–`T4` audio routing (`\|` mixing into `0:a:0`, `t2:`–`t4:` separate tracks), and XDG `GlobalShortcuts` (`zbus`) + X11 fallback + `AppSpec::on_key` (`F9`/`F10`/`F11`). |
| Windows Native Capture & Hotkeys (W01–W07: `a1ec416`, `4f01cc3`, `88d7631`, `aae63b7`) | Implemented & Linux-verified; Windows hardware/CI pending | `windows 0.62` WGC monitor/window capture (`windows_wgc.rs`), WASAPI loopback + mic on dedicated `COINIT_MULTITHREADED` threads (`windows_audio.rs`, isolated from `cpal`'s STA thread), FFmpeg HW (`h264_nvenc`/`amf`/`qsv`) + `libx264` fallback encoder (`windows_encoder.rs`), bounded segmented rolling replay buffer (`windows_replay.rs`), `WindowsCaptureBackend` + `RegisterHotKey` (`windows_backend.rs`, `hotkey.rs`), and Rune UI wiring. |
| Library (`ClipForge/crates/library`, `views/library.rs`) | Verified & passing | SQLite v3 (`ClipOrigin`, `trashed_at`, `original_path`, `.clipforge_trash`), managed-root containment checks, `Sample`/`Imported` disk protection, multi-select, inline confirm/rename, context menu, and 320px Inspector. |
| Media Player, Audio & Mic Test (`ClipForge/crates/media`, `aa6825c`, `aae63b7`) | Verified on Linux (`aa6825c` COM fix) | Off-UI-thread `ffmpeg` RGBA8 pipe + persistent `"clipforge-cpal"` STA thread (`aa6825c` fixes Windows `0xC0000005` WASAPI COM/thread-exit crash) + `AutoMix` multi-track playback, Fast/Precise Trim (`build_trim_args`, 2h benchmark), `MicTestHandle`, and `CREATE_NO_WINDOW` subprocess flags. |
| Desktop Cloud, CI & Portable Packaging (`.github/workflows/`, `packaging/windows/`, `crates/cloud-client`) | Verified on Linux (`8361f62`, `b298761`) | `/api/v1` `HttpCloudClient` + strict `secret-tool` token storage (`1afbd80`), Fast/Precise Trim (`dfadb3d`), split `ci.yml` + `windows-portable.yml` (`8361f62`), and Windows portable ZIP bundling `ClipForge.exe` + `ffmpeg.exe`/`ffprobe.exe` + `THIRD-PARTY-NOTICES.txt` (`b298761`). |
| Standalone `ClipForge-Server` (`ClipForge-Server`, `9216eb2`) | Verified (`9216eb2`); LXC `112` rebuild pending | 5-crate workspace + `web/index.html` + `Dockerfile` + `compose.yaml` + OG/Twitter/oEmbed link previews (`7ef9197`) + storage-first `DELETE` & visibility sync (`9216eb2`). |

## 2. Windows Native Capture & Console Fix (`a1ec416`..`aae63b7`)

- **W01 (`aae63b7`)**: Release builds suppress console window via `#[cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]`, add persistent file logging, and apply `CREATE_NO_WINDOW` (`0x0800_0000`) to all subprocess spawns in `crates/media`, `crates/capture`, and `apps/desktop`.
- **W02 (`4f01cc3`)**: `crates/capture/src/windows_wgc.rs` implements D3D11 + DXGI + `IGraphicsCaptureItemInterop` (`CreateForMonitor` / `CreateForWindow`), `Direct3D11CaptureFramePool::CreateFreeThreaded`, reusable staging texture, `ContentSize` resize handling, `DXGI_ERROR_DEVICE_REMOVED` handling, `ItemClosed`, and `enumerate_windows_monitors`.
- **W03 (`4f01cc3`)**: `crates/capture/src/windows_audio.rs` implements WASAPI `AUDCLNT_STREAMFLAGS_LOOPBACK` (with silence padding) and `eCapture` mic capture on dedicated `COINIT_MULTITHREADED` threads (isolated from `cpal`'s STA thread) + `enumerate_wasapi_devices`.
- **W04 (`88d7631`)**: `crates/capture/src/windows_encoder.rs` probes `h264_nvenc`/`h264_amf`/`h264_qsv` with `libx264` fallback and muxes raw BGRA + `f32le` PCM via `ffmpeg` (`CREATE_NO_WINDOW`).
- **W05 (`88d7631`)**: `crates/capture/src/windows_replay.rs` implements a bounded segmented rolling replay ring (`-f segment -segment_time 2 -segment_wrap <slots>`) and `save_replay()` fast `concat` MP4 export.
- **W06 & W07 (`88d7631`, `aae63b7`)**: `WindowsCaptureBackend` (`create_capture_backend()`), `RegisterHotKey` (`GlobalHotkeyStatus::WindowsGlobalActive`), and truthful Windows Recording/Settings UI wiring.

## 3. Verified Build & Test Results (`aae63b7`, Linux Host)

- `cargo fmt --all -- --check`: **PASS**
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** (0 crate warnings; 3 pre-existing manifest warnings in `Cargo.toml`)
- `XDG_CONFIG_HOME=/tmp/clipforge-test-cfg cargo test --workspace`: **PASS** (138 passed, 0 failed, 3 ignored — `clipforge-capture` 48+3 ignored, `clipforge-cloud-client` 6, `clipforge-common` 3, `clipforge-desktop` 34, `clipforge-library` 7, `clipforge-media` 17 unit + 15 integration, `clipforge-server` 5+3).
- `cargo run -p clipforge-desktop -- --scene recording --snap /tmp/clipforge-recording-wgc.png`: **PASS** (390,547 B, zero Rune a11y/layout warnings; Linux GSR UI preserved).

### Verification Scope Distinction
- **Verified on Linux:** All 138 workspace unit/integration tests (including pure-Rust Windows encoder arg-builder, replay ring-slot math, and `cpal` persistent-thread tests) and `--snap` headless UI rendering.
- **Pending on `windows-latest` CI & Physical Windows 10/11 Hardware:** Live compilation of `#[cfg(target_os = "windows")]` Win32/WinRT modules (`windows_wgc.rs`, `windows_audio.rs`, `RegisterHotKey`), WASAPI `cpal` `0xC0000005` fix verification on Windows Server 2025, real WGC monitor/window capture, WASAPI loopback+mic A/V sync, replay buffer save, `RegisterHotKey`, and portable ZIP execution.
