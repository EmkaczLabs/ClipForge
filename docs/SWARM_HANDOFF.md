# ClipForge Swarm Handoff

## Current Environment & Baseline

- **Host:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.
- **Verified Commit:** `7b3da9a` on `main` (Obsidian Forge UI Overhaul `97844c6..7b3da9a`).
- **Verified Test Suite:** `fmt`, `clippy -D warnings`, `cargo test --workspace` (73 passed, 0 failed, 1 ignored `live_gsr_replay_save_repeated_and_stop`), and all 5 `--scene <name> --snap` renders pass with zero Rune a11y/layout warnings.

## Swarm State

| Member | Role | Ownership | Latest Verified Work |
|---|---|---|---|
| `arch` | Architecture & Integration | `Cargo.toml`, `crates/common/`, `docs/`, `apps/desktop/src/{state,worker,main}.rs` | Implemented shared 5-screen state/config/worker hooks (`97844c6`), fixed 3 text-clipping polish items (`7b3da9a`), and updated `docs/` |
| `design` | Design System & Specs | `docs/design.md`, `docs/ui-screen-specs.md`, `docs/ui-implementation-plan.md` | Canonical Obsidian Forge tokens, Stitch conflict resolutions, and 5-screen specs |
| `shell` | Theme, Components & Shell | `theme.rs`, `components.rs`, `views/mod.rs` | Obsidian Forge palette, SVG icon & primitive library, 48px topbar, 64px rail, 28px status bar (`e107a77`) |
| `library-ui` | Clip Library View | `views/library.rs` | Filter/sort/search/game-filter, multi-select, inline confirm/rename, context menu, 320px Inspector (`43897b9`) |
| `editor-ui` | Video Editor View | `views/editor.rs` | 16:9 viewport, transport/audio/track bar, zoomable timeline, Clip & Export inspector (`09ba361`, `7b3da9a`) |
| `recording-ui` | Recording Studio View | `views/recording.rs` | Live GSR controls, `DiscoverCaptureSources`, Wayland `FocusedWindow` guard, codec notices (`6610522`, `7b3da9a`) |
| `sharing-ui` | Shared & Cloud View | `views/uploads.rs` | Disconnected connect/health-check & Connected shares/uploads/copy/open/remote-delete (`aecfb6e`) |
| `settings-ui` | Settings View | `views/settings.rs` | 8-tab category rail, search, `AppConfig` persistence, Reset/Save (`83baeae`, `7b3da9a`) |
| `qa` | QA & Security | Read-only verification | Verified `fmt`, `clippy`, 73+1 ignored tests, and 5-screen `--snap` visual inspection (#96) |

## Remaining Limitations & Follow-Ups

1. **Interactive Windowed Verification:** PipeWire/ALSA audio auditioning, live pointer-drag timeline/volume/zoom UX, and live server sharing remain to be tested interactively (`cargo run -p clipforge-desktop`).
2. **Wayland Global Hotkeys:** `HotkeyManager::start_x11` is X11-only; Wayland needs XDG `GlobalShortcuts` portal support (documented truthfully in `views/recording.rs` & `views/settings.rs`).
3. **Sample origin migration & CI/M6:** `apps/desktop` can switch from `"sample"` tag to `ClipOrigin::Sample`; `.github/workflows/ci.yml` needs `libxkbcommon-dev` + `libasound2-dev`. See `docs/NEXT_TASKS.md`.
