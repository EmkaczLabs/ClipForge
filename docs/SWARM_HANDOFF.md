# ClipForge Swarm Handoff

## Current Environment & Baseline

- **Host:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.
- **Verified Commit:** `56099f1` on `main` (Obsidian Forge UI Overhaul `97844c6..7b3da9a` + PipeWire device list & Wayland `GlobalShortcuts` regression fixes `aac7e1b..56099f1`).
- **Verified Test Suite:** `fmt`, `clippy -D warnings`, `cargo test --workspace` (82 passed, 0 failed, 2 ignored `live_gsr_replay_save_repeated_and_stop` & `live_wayland_portal`), and `--scene recording --snap /tmp/clipforge-recording-fix.png` all passing.

## Swarm State

| Member | Role | Ownership | Latest Verified Work |
|---|---|---|---|
| `arch` | Architecture & Integration | `Cargo.toml`, `crates/common/`, `docs/`, `views/recording.rs` polish | Added `zbus` (`aac7e1b`), wired `GlobalHotkeyStatus` badge/notice in `views/recording.rs` (`56099f1`), and updated `docs/` |
| `audio-ui` | Recording Audio UI | `apps/desktop/src/views/recording.rs` | Fixed PipeWire device/app-stream text overlap, added bounded scroll + `rec-audio-inspector` + 4-device `rune::Runtime` tests (`38e8624`) |
| `hotkeys` | Global & Focused Hotkeys | `crates/capture/src/hotkey.rs`, `apps/desktop/src/{state,worker,main}.rs` | Implemented XDG `GlobalShortcuts` via `zbus` + X11 fallback + `AppSpec::on_key` `F9`/`F10`/`F11` + `GlobalHotkeyStatus` (`3af3d94`) |
| `qa` | QA & Security | Read-only verification | Verified `fmt`, `clippy`, 82+2 ignored tests, `/tmp/clipforge-recording-fix.png`, and `zbus` portal security/cleanup (#108) |

## Remaining Limitations & Follow-Ups

1. **Unfocused Wayland Hotkey & Interactive Windowed Verification:** Live KDE Wayland portal permission approval + unfocused `F9`/`F10`/`F11` delivery, PipeWire/ALSA audio auditioning, pointer-drag timeline/volume/zoom UX, and live server sharing remain to be tested interactively (`cargo run -p clipforge-desktop`).
2. **Sample origin migration & CI/M6:** `apps/desktop` can switch from `"sample"` tag to `ClipOrigin::Sample`; `.github/workflows/ci.yml` needs `libxkbcommon-dev` + `libasound2-dev`. See `docs/NEXT_TASKS.md`.
