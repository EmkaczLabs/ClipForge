# ClipForge Swarm Handoff

## Current Environment & Baseline

- **Host:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.
- **Verified Commit:** `ea43ff0` on `main` (`6bd4908` SQLite v3 trash/restore/delete/rename/filter in `crates/library`, `ea43ff0` Library management UI & worker wiring in `apps/desktop`).
- **Verified Test Suite:** `fmt`, `clippy -D warnings`, 51 non-desktop tests + 1 ignored live GSR test + 14 desktop tests + `--snap` render (`/tmp/clipforge-m1.png`, 194,513 bytes) all passing.

## Swarm State

| Member | Role | Ownership | Latest Verified Work |
|---|---|---|---|
| `arch` | Architecture & Integration | `Cargo.toml`, `crates/common/`, `docs/`, `packaging/` | Aligned `crates/library` v3 + `rune` APIs (#44); updated `docs/` for `ea43ff0` |
| `lib-store` | Library Storage | `crates/library/` | Implemented SQLite v3 trash/restore, permanent delete, managed-root validation, `Sample`/`Imported` protection, rename, and trash/sort filtering (`6bd4908`) |
| `lib-ui` | Desktop Library Management | `apps/desktop/` | Implemented trash/restore/delete confirmation, Ctrl/Shift multi-selection + bulk actions, inline rename (`rune::text_input`), Open Containing Folder, sort/filter, and floating context menu (`ea43ff0`) |
| `qa` | QA & Security | Read-only verification | Verified `ea43ff0` full suite (`fmt`, `clippy`, 51+1 ignored, 14 desktop, `--snap` 194,513 bytes) and security review (#49) |

## Remaining Limitations & Follow-Ups

1. **Interactive Windowed Verification:** Live Wayland windowed pointer-drag timeline/trim-handle UX, multi-select/context-menu interaction, and multi-frame playback in a window should be tested interactively (`cargo run -p clipforge-desktop`).
2. **Wayland `FocusedWindow` & global hotkeys:** `CaptureTarget::FocusedWindow` (`-w focused`) and `HotkeyManager::start_x11` are X11-only; Wayland needs UI disable/warn for `FocusedWindow` and XDG `GlobalShortcuts` portal support.
3. **Sample origin migration:** `apps/desktop` still tags sample clips with `"sample"` and can switch to `ClipOrigin::Sample` from `crates/library`.
4. **CI & M5/M6 validation:** `.github/workflows/ci.yml` needs `libxkbcommon-dev` on Ubuntu runners; live desktop-to-server sharing and `packaging/` builds remain to be executed. See `docs/NEXT_TASKS.md`.
