# ClipForge Swarm Handoff

## Current Environment & Baseline

- **Host:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.
- **Verified Commit:** `a30157e` on `main` (`19f0821` capture fixes, `334ea11` library/media fixes, `a30157e` desktop fixes).
- **Verified Test Suite:** 45 non-desktop tests + 1 ignored live GSR test (`live_gsr_replay_save_repeated_and_stop`) + 7 desktop tests + `--snap` render all passing, with `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` clean.

## Swarm State

| Member | Role | Ownership | Latest Verified Work |
|---|---|---|---|
| `arch` | Architecture & Integration | `Cargo.toml`, `crates/common/`, `docs/`, `packaging/` | Updated `docs/PROGRESS.md`, `docs/NEXT_TASKS.md`, `docs/SWARM_HANDOFF.md` for CachyOS verification (`a30157e`) |
| `ui` | Rune Desktop UI | `apps/desktop/` | Startup timing instrumentation, immediate cached clip emission, `SaveReplay` SQLite indexing, `OpenInPlayer`, sample clip guards/badges, capture error recovery (`63f8a18` / `a30157e`) |
| `gsr` | GSR Integration | `crates/capture/` | Fixed `-bm auto` & `very_high`, deduplicated `CLIPFORGE_SAVED:`, stderr error capture, live `SIGUSR1` replay test on GSR 6.1.7 (`a1529cf` / `19f0821`) |
| `media` | Media & Library | `crates/media/`, `crates/library/` | SQLite v2 `ClipOrigin` schema, canonical path lookup, `ffmpeg` past-EOF thumbnail fallback (`ec3f4d7` / `334ea11`) |
| `cloud` | Cloud & Backend | `crates/cloud-client/`, `apps/server/`, `apps/web/` | Axum server, auth/API keys, streaming upload, `Range` seeking, HTML5 player |
| `qa` | QA & Security | Read-only verification | Verified `a30157e` full suite, `--snap`, and security review (#11) |

## Resolved Blockers (Previously on Linux Mint)

- **Desktop linking (`-lxkbcommon`):** Resolved on CachyOS; `clipforge-desktop` links, passes 7 tests, and renders `--snap`.
- **Live GSR capture:** Resolved on CachyOS with `gpu-screen-recorder 6.1.7` on KDE Wayland + NVIDIA RTX 3070.

## Remaining Limitations & Follow-Ups

1. **Wayland `FocusedWindow` & global hotkeys:** `CaptureTarget::FocusedWindow` (`-w focused`) and `HotkeyManager::start_x11` are X11-only; Wayland needs UI disable/warn for `FocusedWindow` and XDG `GlobalShortcuts` portal support.
2. **Sample origin migration:** `apps/desktop` still tags sample clips with `"sample"` and can switch to `ClipOrigin::Sample` from `crates/library`.
3. **CI runner dependency:** `.github/workflows/ci.yml` still needs `libxkbcommon-dev` on Ubuntu runners to link `clipforge-desktop`.
4. **M5/M6 end-to-end validation:** Live desktop-to-server sharing flow and `packaging/` builds (`PKGBUILD`, Docker, CI) remain to be executed. See `docs/NEXT_TASKS.md`.
