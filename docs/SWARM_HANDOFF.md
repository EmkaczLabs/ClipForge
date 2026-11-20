# ClipForge Swarm Handoff

## Current Environment & Baseline

- **Host:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.
- **Verified Commit:** `40311c8` on `main` (`e372527` `cpal 0.15` workspace dep, `8abb686` `crates/media` audio + A/V sync + bounded queue + export progress/cancel, `5e3f3eb` desktop state/worker/library/keyboard wiring, `40311c8` editor audio/zoom/export UI).
- **Verified Test Suite:** `fmt`, `clippy -D warnings`, `cargo test --workspace` (71 passed, 0 failed, 1 ignored `live_gsr_replay_save_repeated_and_stop`), and `--snap /tmp/clipforge-m1.png` (163,587 bytes) all passing.

## Swarm State

| Member | Role | Ownership | Latest Verified Work |
|---|---|---|---|
| `arch` | Architecture & Integration | `Cargo.toml`, `crates/common/`, `docs/`, `packaging/` | Added `cpal = "0.15"` (`e372527`), aligned `crates/media` <-> `apps/desktop` contracts (#61), updated `docs/` for `40311c8` |
| `decoder` | Video Decoding & Audio | `crates/media/` | Benchmarked 1080p60 decode (609.8 fps scaled / 286.0 fps native), added bounded `mpsc` queue, `cpal 0.15` audio + `samples_played` A/V sync + wall-clock fallback, `set_audio_track`, `ExportCancelToken` + `trim_with_progress` (`8abb686`) |
| `renderer` | Rune Renderer & Wiring | `apps/desktop/src/{main,state,worker,views/library}.rs` | Wired `play_in_editor`, `set_volume`, `toggle_mute`, `set_timeline_zoom`, `cancel_export`, `ClipOrigin::Trimmed` export indexing, and `AppSpec::on_key` (`5e3f3eb`) |
| `timeline` | Editor Timeline & Controls | `apps/desktop/src/views/editor.rs` | Added mute/volume slider, `1x..8x` zoom + `scroll_x`, `on_key` shortcuts, and `export-progress-box` + `Cancel Export` (`40311c8`) |
| `qa` | QA & Security | Read-only verification | Verified `40311c8` full suite (`fmt`, `clippy`, 71+1 ignored, `--snap` 163,587 bytes) and subprocess/buffering safety review (#66) |

## Remaining Limitations & Follow-Ups

1. **Manual Windowed Verification:** PipeWire/ALSA audio auditioning and live pointer-drag timeline/volume/zoom UX in a Wayland window remain to be verified interactively (`cargo run -p clipforge-desktop`).
2. **Wayland `FocusedWindow` & global hotkeys:** `CaptureTarget::FocusedWindow` (`-w focused`) and `HotkeyManager::start_x11` are X11-only; Wayland needs UI disable/warn for `FocusedWindow` and XDG `GlobalShortcuts` portal support.
3. **Sample origin migration:** `apps/desktop` still tags sample clips with `"sample"` and can switch to `ClipOrigin::Sample` from `crates/library`.
4. **CI & M5/M6 validation:** `.github/workflows/ci.yml` needs `libxkbcommon-dev` and `libasound2-dev` (ALSA dev headers for `cpal`) on Ubuntu runners; live desktop-to-server sharing and `packaging/` builds remain to be executed. See `docs/NEXT_TASKS.md`.
