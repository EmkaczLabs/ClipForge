# ClipForge Progress Checkpoint

**Checkpoint Timestamp:** 2026-11-20T23:16:00+01:00  
**Latest Verified Commit:** `56099f1` (`main`)  
**Host Environment:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.

## Milestone Status Summary

| Milestone | Status | Notes |
|---|---|---|
| M1 — Foundation & Obsidian Forge UI (`97844c6..7b3da9a`) | Verified & passing | 5-screen `NavSection` (`Library` default, `Editor`, `Recording`, `Uploads`, `Settings`), Obsidian Forge palette (`theme.rs`), SVG icon & primitive catalog (`components.rs`), 48px topbar + 64px rail + 28px status bar (`views/mod.rs`), all 5 `--scene <name> --snap` renders pass. |
| M2 — Recording Studio & Global Hotkeys (`38e8624`, `3af3d94`, `56099f1`) | Verified (automated + live portal bind) | PipeWire audio device & app-stream list overlap fixed (`views/recording.rs` `38e8624`: stacked label + technical ID with `.truncate()`/`.truncate_middle()`, bounded `rec-audio-list` scroll, `rec-audio-inspector`, tested at `1280x800` & `960x640`). Global hotkeys (`3af3d94`, `56099f1`): XDG Desktop Portal `GlobalShortcuts` via `zbus` (`CreateSession`, `BindShortcuts`, `Activated`, `Session::Close`) + X11 `start_x11` fallback + `AppSpec::on_key` focused `F9`/`F10`/`F11` + truthful `GlobalHotkeyStatus` badge & notice in `views/recording.rs`. Live Wayland portal bind verified (`live_wayland_portal` -> `PermissionPending` awaiting KDE dialog approval); unfocused Wayland `F9`/`F10`/`F11` delivery remains to be manually verified after approving the KDE portal prompt. |
| M3 — Clip Library (`crates/library`, `views/library.rs`) | Verified & passing | SQLite v3 trash/restore (`trashed_at`, `original_path`, `.clipforge_trash`), managed-root boundary validation, `Sample`/`Imported` disk protection, `rename_clip`, game filter, Ctrl/Shift multi-select + bulk actions, inline confirm/rename, floating context menu, and 320px Inspector. |
| M4 — Video Editor & Playback (`crates/media`, `views/editor.rs`) | Implemented & tested (headless) | `VideoPlayerHandle` (`ffmpeg` RGBA8 pipe, bounded `mpsc` queue, `cpal 0.15` PipeWire/ALSA audio + `samples_played` A/V sync, wall-clock fallback, `set_audio_track`), `ExportCancelToken` + `trim_with_progress`, 16:9 viewport, transport + volume/mute + `audio_track` selector, `1x..8x` zoomable timeline + scrub/trim handles, and Clip & Export inspector. |
| M5 — Shared & Cloud (`apps/server`, `crates/cloud-client`, `views/uploads.rs`) | Implemented & tested (unit/mock + server integration) | Disconnected onboarding/connect/health-check and Connected `remote_shares` + `uploads` management with retry/cancel, `CopyToClipboard`, `OpenUrl`, and remote delete confirmation. |
| M6 — Settings & Release (`views/settings.rs`, `packaging/`) | Settings verified; packaging pending | 8-tab category rail, search filter, `AppConfig` persistence (`~/.config/clipforge/config.json`), `Reset`/`Save Settings`. `packaging/` (`Dockerfile`, `PKGBUILD`, `deb/`) remains to be built/validated. |

## Targeted Regression Fixes (`aac7e1b..56099f1`)

1. **PipeWire Device List Text Overlap (`38e8624` in `apps/desktop/src/views/recording.rs`):**
   - Stacked human-readable device name and technical source ID inside `col().grow(1.0).min_w(0.0)` with `.truncate()` / `.truncate_middle()` and `.clip_rounded(radius::SM)`.
   - Bounded `rec-audio-list` (`max_h(260.0)` + scroll) and added `rec-audio-inspector` for full wrapped active routing details.
   - Added `rune::Runtime` render tests for the 4 long PipeWire names at `1280x800` and `960x640` + checkbox toggle.
2. **Global & Focused Hotkeys (`aac7e1b`, `3af3d94`, `56099f1`):**
   - Added `zbus = "5"` (`tokio`) to root `Cargo.toml` (`aac7e1b`).
   - `crates/capture/src/hotkey.rs`: Added `HotkeyAction::ToggleReplayBuffer`, `GlobalHotkeyStatus` (`WaylandPortalActive`, `PermissionPending`, `X11GlobalActive`, `FocusedWindowOnly`, `RegistrationError`), `select_hotkey_backend()`, `HotkeySpec::to_xdg_trigger()`, and XDG `GlobalShortcuts` portal session via `zbus` with X11 `start_x11` fallback.
   - `apps/desktop/src/{worker.rs,state.rs,main.rs,views/recording.rs}`: Wired `HotkeyManager::start_auto` on `Initialize` and re-registration on `SaveConfig`; wired `AppSpec::on_key` for focused `F9`/`F10`/`F11` across all views; rendered `s.hotkey_status` badge & detail notice in `hotkeys_card` (`56099f1`).

## Latest Verified Test Results (`56099f1`)

- `cargo fmt --all -- --check`: **PASS**
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** (0 crate warnings; 3 pre-existing workspace manifest warnings in `Cargo.toml`)
- `cargo test --workspace`: **PASS** (82 passed, 0 failed, 2 ignored):
  - `clipforge-capture`: 25 passed, 2 ignored (`live_gsr_replay_save_repeated_and_stop`, `live_wayland_portal`)
  - `clipforge-cloud-client`: 2 passed
  - `clipforge-common`: 3 passed
  - `clipforge-desktop`: 21 passed
  - `clipforge-library`: 7 passed (integration)
  - `clipforge-media`: 16 passed (7 unit + 9 integration)
  - `clipforge-server`: 8 passed (5 unit + 3 integration)
- `cargo run -p clipforge-desktop -- --scene recording --snap /tmp/clipforge-recording-fix.png`: **PASS** (zero Rune a11y/layout warnings; visually inspected).

### Remaining Manual Verification Steps
- Launch `cargo run -p clipforge-desktop` in KDE Wayland, approve the KDE `GlobalShortcuts` portal permission dialog, and verify unfocused `F9` (Save Replay), `F10` (Toggle Replay Buffer), and `F11` (Start/Stop Recording) triggers alongside PipeWire/ALSA audio auditioning and pointer-drag timeline/trim UX.
- Run live `cargo test -p clipforge-capture -- --ignored live_gsr_replay_save_repeated_and_stop` and test live desktop-to-server sharing against `clipforge-server`.
