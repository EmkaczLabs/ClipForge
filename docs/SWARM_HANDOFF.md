# ClipForge Swarm Handoff

## Current Environment & Baseline

- **Host:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.
- **Verified Commit:** `18cd8cb` on `main` (`112ccc7` off-UI-thread RGBA8 video player, `28d0093` Library interactions & playback wiring, `5fe80d1` + `18cd8cb` interactive Editor viewport/timeline/trimming).
- **Verified Test Suite:** `fmt`, `clippy -D warnings`, 48 non-desktop tests + 1 ignored live GSR test + 9 desktop tests + `--snap` render (`/tmp/clipforge-m1.png`, 162,616 bytes) all passing.

## Swarm State

| Member | Role | Ownership | Latest Verified Work |
|---|---|---|---|
| `arch` | Architecture & Integration | `Cargo.toml`, `crates/common/`, `docs/`, `packaging/` | Aligned `VideoPlayerHandle` + `EditorState` / `rune::Pixels` interface (#23); updated `docs/` for `18cd8cb` |
| `media-dev` | Media Video Player | `crates/media/` | Implemented `VideoPlayerHandle`, `DecodedFrame` (RGBA8), `extract_timeline_thumbnails`, and 6 new real-MP4 tests (`112ccc7`) |
| `ui-dev` | Desktop Library & Wiring | `apps/desktop/src/{main,state,worker,views/library}.rs` | Wired `views/library.rs` click/double-click/detail preview/actions, `EditorState`, and off-thread `VideoPlayerHandle` -> `rune::Pixels::set` (`28d0093`) |
| `editor-dev` | Desktop Video Editor | `apps/desktop/src/views/editor.rs` | Implemented `El::pixels_fit` viewport, clickable/draggable timeline & trim handles, timeline thumbnails, timecode, and export progress/error UI (`5fe80d1`, `18cd8cb`) |
| `qa` | QA & Security | Read-only verification | Verified `18cd8cb` full suite (`fmt`, `clippy`, 48+1 ignored, 9 desktop, `--snap` 162,616 bytes) (#35) |

## Remaining Limitations & Follow-Ups

1. **Interactive Windowed Verification:** Live Wayland windowed pointer-drag timeline/trim-handle UX and multi-frame playback in a window are not covered by headless `--snap` / `rune::Runtime` unit tests and should be tested interactively (`cargo run -p clipforge-desktop`).
2. **Wayland `FocusedWindow` & global hotkeys:** `CaptureTarget::FocusedWindow` (`-w focused`) and `HotkeyManager::start_x11` are X11-only; Wayland needs UI disable/warn for `FocusedWindow` and XDG `GlobalShortcuts` portal support.
3. **Sample origin migration:** `apps/desktop` still tags sample clips with `"sample"` and can switch to `ClipOrigin::Sample` from `crates/library`.
4. **CI & M5/M6 validation:** `.github/workflows/ci.yml` needs `libxkbcommon-dev` on Ubuntu runners; live desktop-to-server sharing and `packaging/` builds remain to be executed. See `docs/NEXT_TASKS.md`.
