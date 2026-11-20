# ClipForge Progress Checkpoint

**Checkpoint Timestamp:** 2026-11-20T23:51:00+01:00  
**Latest Verified Commit:** `1592c5a` (`main`)  
**Host Environment:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.

## Milestone Status Summary

| Milestone | Status | Notes |
|---|---|---|
| M1 — Foundation & Obsidian Forge UI (`97844c6..7b3da9a`) | Verified & passing | 5-screen `NavSection` (`Library` default, `Editor`, `Recording`, `Uploads`, `Settings`), Obsidian Forge palette (`theme.rs`), SVG icon & primitive catalog (`components.rs`), 48px topbar + 64px rail + 28px status bar (`views/mod.rs`), all 5 `--scene <name> --snap` renders pass. |
| M2 — Recording Studio, Audio Routing & Hotkeys (`8ff12e8`, `636fce3`, `1592c5a`) | Verified (live GSR + `ffprobe`) | Deterministic `Auto` vs `T1`–`T4` GSR audio routing (`crates/capture/src/audio.rs`): `Auto`/`T1` mix via `|` (`-a "default_output|default_input"`) into `0:a:0`; `t2:`–`t4:` emit separate `-a` tracks. Live GSR 6.1.7 + `ffprobe` verified all 5 routing cases (`live_audio_routing` 1/1 PASS). `views/recording.rs` adds per-source `Auto`/`T1`–`T4` pills, `track_diagnostics_panel`, `audio_restart_banner`, and `mic_test_card`. Global hotkeys use XDG `GlobalShortcuts` (`zbus`) + X11 fallback + `AppSpec::on_key`. |
| M3 — Clip Library (`crates/library`, `views/library.rs`) | Verified & passing | SQLite v3 trash/restore (`trashed_at`, `original_path`, `.clipforge_trash`), managed-root boundary validation, `Sample`/`Imported` disk protection, `rename_clip`, game filter, Ctrl/Shift multi-select + bulk actions, inline confirm/rename, floating context menu, and 320px Inspector. |
| M4 — Video Editor, Multi-Track Audio & Mic Test (`3674bce`, `636fce3`) | Verified (automated + PipeWire unit/integration) | `crates/media` (`3674bce`) defaults `audio_track_index` to `None` (`Auto`) using `AudioTrackMode::AutoMix` (`amix` when `track_count > 1`) so multi-track recordings play all tracks in `Auto` mode. `MicTestHandle` (`crates/media/src/mic_test.rs`) provides standalone `pactl` + `parec` + `cpal` mic testing with real dBFS metering, 3s sample record/playback, no silent device fallback, and `kill_on_drop(true)` cleanup. |
| M5 — Shared & Cloud (`apps/server`, `crates/cloud-client`, `views/uploads.rs`) | Implemented & tested (unit/mock + server integration) | Disconnected onboarding/connect/health-check and Connected `remote_shares` + `uploads` management with retry/cancel, `CopyToClipboard`, `OpenUrl`, and remote delete confirmation. |
| M6 — Settings & Release (`views/settings.rs`, `packaging/`) | Settings verified; packaging pending | 8-tab category rail, search filter, `AppConfig` persistence (`~/.config/clipforge/config.json`), `Reset`/`Save Settings`. `packaging/` (`Dockerfile`, `PKGBUILD`, `deb/`) remains to be built/validated. |

## Automatic Audio Routing & Microphone Test (`8ff12e8`..`1592c5a`)

1. **Root Cause & Deterministic Audio Routing (`crates/capture/src/audio.rs`, `crates/media/src/{audio,player}.rs`):**
   - **Capture root cause:** `args.rs` previously emitted separate `-a <src>` flags for every `audio_sources` entry, causing GSR 6.1.7 to write separate streams (`0:a:0` system, `0:a:1` mic).
   - **Playback root cause:** `player.rs` previously defaulted `audio_track_index` to `Some(0)` (leaving `0:a:1` silent) and skipped `AudioSession::start` when `audio_track` was `None` (`Auto`).
   - **Fix:** `crates/capture/src/audio.rs` (`8ff12e8`) introduces `AudioTrackTarget::{Auto, T1..T4}`, `AudioRoutingPlan`, `resolve_audio_routing`, `parse_routed_audio_source`, and `format_routed_audio_source` — `Auto`/`T1` mix sources with `|` into `0:a:0`, while `t2:`–`t4:` emit separate `-a` tracks. `crates/media` (`3674bce`) defaults `audio_track_index` to `None` (`Auto`) and adds `AudioTrackMode::AutoMix` (`amix` for multi-track files) and `AudioTrackMode::Single(idx)`.
2. **Standalone PipeWire Microphone Test (`crates/media/src/mic_test.rs`, `3674bce`):**
   - `MicTestHandle` (`pactl` + `parec` + `cpal`) captures from the exact selected PipeWire/Pulse source without touching the active GSR replay buffer, streams real-time RMS/peak dBFS (`DBFS_FLOOR = -96.0`), records bounded samples (3s default), cleans up `parec`/`cpal` via `kill_on_drop(true)` + `Drop`, plays back resampled PCM via `cpal`, and emits `MicDiagnosticSummary` (`Silence`, `MutedOrVeryQuiet`, `GoodSignal`, `Clipping`, `MissingDevice`, `PermissionError`, `Success`).
3. **Desktop Wiring & Recording Studio UI (`636fce3`, `1592c5a`):**
   - `state.rs` & `worker.rs` (`636fce3`): Wired `MicTestHandle` (`WorkerCommand::{StartMicTest, StartMicMonitor, StopMicTest, PlayMicTestSample, ClearMicTest, RestartReplayBuffer}`), `audio_source_track` / `set_audio_source_track`, `audio_config_dirty_while_active`, `audio_diagnostics()`, and `MicTestState`.
   - `views/recording.rs` (`1592c5a`): Added per-source `Auto`/`T1`–`T4` pills in `audio_source_row`, `audio_restart_banner`, `track_diagnostics_panel`, and `mic_test_card`.

## Latest Verified Test Results (`1592c5a`)

- `cargo fmt --all -- --check`: **PASS**
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** (0 crate warnings; 3 pre-existing workspace manifest warnings in `Cargo.toml`)
- `cargo test --workspace`: **PASS** (98 passed, 0 failed, 3 ignored):
  - `clipforge-capture`: 31 passed, 3 ignored (`live_gsr_replay_save_repeated_and_stop`, `live_wayland_portal`, `live_audio_routing`)
  - `clipforge-cloud-client`: 2 passed
  - `clipforge-common`: 3 passed
  - `clipforge-desktop`: 23 passed
  - `clipforge-library`: 7 passed (integration)
  - `clipforge-media`: 24 passed (13 unit + 11 integration)
  - `clipforge-server`: 8 passed (5 unit + 3 integration)
- `cargo test -p clipforge-capture --test live_audio_routing -- --ignored --nocapture`: **PASS** (1/1 in 9.06s; verified real GSR 6.1.7 + `ffprobe` across all 5 routing cases: system only, mic only, system+mic Auto [1 mixed stream], system+mic T2 [2 streams], system+mic T3/T4 [3 streams]).
- `cargo run -p clipforge-desktop -- --scene recording --snap /tmp/clipforge-studio-ui.png`: **PASS** (390,383 B, zero Rune a11y/layout warnings; visually inspected).

### Remaining Manual Interactive Auditioning
- Launch `cargo run -p clipforge-desktop` in KDE Wayland to interactively verify:
  1. `Test Microphone` with live human voice + audible `Play Sample` playback.
  2. Approving the KDE `GlobalShortcuts` portal dialog and testing unfocused `F9`/`F10`/`F11` delivery.
  3. Live multi-frame editor playback + pointer-drag timeline/trim/volume UX.
