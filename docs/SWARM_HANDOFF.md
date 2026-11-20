# ClipForge Swarm Handoff

## Current Environment & Baseline

- **Host:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.
- **Verified Commit:** `1592c5a` on `main` (`8ff12e8` deterministic Auto/T1-T4 GSR audio routing, `3674bce` AutoMix multi-track playback + `MicTestHandle`, `636fce3` desktop state/worker wiring, `1592c5a` Recording Studio track routing/diagnostics/Mic Test UI).
- **Verified Test Suite:** `fmt`, `clippy -D warnings`, `cargo test --workspace` (98 passed, 0 failed, 3 ignored), `live_audio_routing` (1/1 across all 5 real GSR + `ffprobe` cases), and `--scene recording --snap /tmp/clipforge-studio-ui.png` (390,383 B) all passing.

## Swarm State

| Member | Role | Ownership | Latest Verified Work |
|---|---|---|---|
| `arch` | Architecture & Integration | `Cargo.toml`, `crates/common/`, `docs/`, `apps/desktop/src/{state,worker}.rs` | Wired `crates/capture` routing/diagnostics + `MicTestHandle` + `RestartReplayBuffer` (`636fce3`), and updated `docs/` |
| `audio-capture` | GSR Audio Routing | `crates/capture/` | Added `crates/capture/src/audio.rs` (`Auto`/`T1` `\|` mixing, `t2:`–`t4:` tracks, diagnostics) + 5-case live GSR/`ffprobe` verification (`8ff12e8`) |
| `mic-backend` | Media & Mic Test | `crates/media/` | Added `AudioTrackMode::AutoMix` multi-track playback and `MicTestHandle` (`pactl`+`parec`+`cpal`, dBFS, 3s record/playback, `kill_on_drop`) (`3674bce`) |
| `studio-ui` | Recording Studio UI | `apps/desktop/src/views/recording.rs` | Added `Auto`/`T1`–`T4` track pills, `audio_restart_banner`, `track_diagnostics_panel`, and `mic_test_card` (`1592c5a`) |
| `qa` | QA & Security | Read-only verification | Verified `fmt`, `clippy`, 98+3 ignored tests, 5-case `live_audio_routing`, `/tmp/clipforge-studio-ui.png`, and `parec`/`cpal` cleanup (#125) |

## Remaining Limitations & Follow-Ups

1. **Interactive Windowed Auditioning:** Live voice `Test Microphone` + audible `Play Sample`, KDE `GlobalShortcuts` portal approval + unfocused `F9`/`F10`/`F11`, pointer-drag editor UX, and live server sharing remain to be tested interactively (`cargo run -p clipforge-desktop`).
2. **Sample origin migration & CI/M6:** `apps/desktop` can switch from `"sample"` tag to `ClipOrigin::Sample`; `.github/workflows/ci.yml` needs `libxkbcommon-dev` + `libasound2-dev`. See `docs/NEXT_TASKS.md`.
