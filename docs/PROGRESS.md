# ClipForge Progress Checkpoint

**Checkpoint Timestamp:** 2026-11-20T19:55:00+01:00  
**Latest Verified Commit:** `ea43ff0` (`main`)  
**Host Environment:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.

## Milestone Status Summary

| Milestone | Status | Notes |
|---|---|---|
| M1 — Foundation (Workspace & Rune UI) | Verified & passing | `clipforge-desktop` links and runs on CachyOS Wayland. Startup timing: `setup_worker` `0.062 ms`, `WorkerCommand::Initialize` `13.99 ms`, headless `--snap` `gpu init 341.8 ms` / `first frame 225.31 ms` (`/tmp/clipforge-m1.png`, 194,513 bytes), windowed Wayland first `wl_surface.attach` ~2.0s. `scan_and_load_library` emits cached SQLite clips immediately before probing new files. |
| M2 — Recording (`crates/capture`) | Verified with live GSR 6.1.7 | Fixed GSR 6.1.7 CLI args (`-bm auto`, `very_high`), deduplicated `ReplaySaved` via `has_save_hook` (`CLIPFORGE_SAVED:`), captured stderr errors into `CaptureStatus::Error`, and added `Restart Replay Buffer` / `Clear Error` UI recovery. Live `live_gsr_replay_save_repeated_and_stop` test passed (2 distinct MP4s via `SIGUSR1`, clean `SIGINT` stop). Remaining gaps: `FocusedWindow` (`-w focused`) and global hotkeys are X11-only; `Av1` (on RTX 3070) / `Flac` emit GSR fallback warnings. |
| M3 — Library & Management (`crates/library`, `crates/media`, `apps/desktop`) | Verified & passing | SQLite v3 adds `trashed_at`, `original_path`, `idx_clips_trashed_at`, `.clipforge_trash`, `move_to_trash`, `restore_from_trash`, `permanently_delete`, `rename_clip`, and `trashed_only`/`exclude_trashed` filtering with strict managed-root boundary validation and `Sample`/`Imported` disk protection. `views/library.rs` provides click/double-click selection, Ctrl/Shift multi-selection + bulk actions, inline confirmation (`lib-confirm-bar`), inline rename (`rune::text_input`), Open Containing Folder (`xdg-open`), search/filter/sort, and floating Rune context menu (`on_secondary_press` + `...`). |
| M4 — Editing & Embedded Playback (`crates/media`, `apps/desktop`) | Implemented & tested (headless) | `crates/media/src/player.rs` (`VideoPlayerHandle`) decodes H.264/video off-UI-thread via `ffmpeg -f rawvideo -pix_fmt rgba pipe:1` (`kill_on_drop(true)`), delivering `DecodedFrame` RGBA8 buffers into `rune::Pixels::set` (`El::pixels_fit(..., SurfaceFit::Fit)`). `views/editor.rs` renders the embedded viewport, timeline thumbnails, clickable/draggable playhead and trim-in/out handles, `MM:SS.cc` timecode, trim-range playback, `Open in Player`, and `exporting` / `export_error` UI. Live interactive windowed Wayland pointer-drag UX and multi-frame window playback remain to be manually verified. |
| M5 — Self-Hosted Sharing (`apps/server`, `crates/cloud-client`, `apps/web`) | Implemented & tested | 10 tests pass (auth, API keys, streaming multipart upload, HTTP `Range` seeking, public/private shares, `413` limit). End-to-end desktop-to-server live test pending. |
| M6 — Public Release (`packaging/`) | Scaffolding added | `Dockerfile`, `docker-compose.yml`, `PKGBUILD`, `deb/` templates created. Not yet built/validated end-to-end. |

## Clip Library Management (`6bd4908`, `ea43ff0`)

1. **SQLite v3 & Safe Filesystem Operations (`crates/library`, `6bd4908`):**
   - Migration v3 adds `trashed_at TEXT`, `original_path TEXT`, and `idx_clips_trashed_at` on `clips`; `TRASH_DIR_NAME = ".clipforge_trash"` (skipped by `DirectoryScanner`).
   - `move_to_trash(id, managed_roots)`: refuses `ClipOrigin::Sample` and `ClipOrigin::Imported`, canonicalizes paths (and nearest existing ancestor for missing files) to enforce strict containment within `managed_roots`, moves to `<managed_root>/.clipforge_trash/<id>-<filename>` with cross-device fallback, and handles missing disk files gracefully (`file_moved: false`).
   - `restore_from_trash(id)`: moves file back to `original_path` (or unique `stem (restored N).ext` on collision), clears `trashed_at`/`original_path`, and returns `NotFound` if the trash file is missing.
   - `permanently_delete(id, managed_roots)`: deletes only the SQLite row for `Sample`/`Imported` (never touches disk files); validates `managed_roots` and removes disk file (graceful on `NotFound`) + DB row for `Recorded`/`Trimmed`.
   - `rename_clip(id, new_title)` trims and rejects empty titles; `ClipListOptions` supports `trashed_only` and `exclude_trashed` (default `true`).
2. **Desktop Library Management UI (`apps/desktop`, `ea43ff0`):**
   - Inline confirmation bar (`lib-confirm-bar`) for Move to Trash, Restore from Trash, and Permanent Delete; closes `VideoPlayerHandle` if the active clip is trashed/deleted.
   - Multi-selection via Ctrl-click and Shift-click (`on_press_tx` / `tx.mods()`) plus selection checkboxes (`select-check-{id}`), Select All / Clear, Bulk Favorite, Bulk Move to Trash, Bulk Restore, and Bulk Permanent Delete.
   - Inline rename via `rune::text_input` + `TextEdit`, Open Containing Folder (`xdg-open` after `parent.is_dir()` check), search, filters (`All`, `Recorded`, `Trimmed`, `Favorites`, `Shared`, `Trash`), sorting (`Newest`, `Oldest`, `Longest`, `Largest`, `Title`), and floating Rune context menu (`on_secondary_press` + `...`).

## Latest Verified Test Results (`ea43ff0`)

- `cargo fmt --all -- --check`: **PASS**
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** (0 crate warnings; 3 pre-existing workspace manifest warnings in `Cargo.toml`)
- `cargo test --workspace --exclude clipforge-desktop`: **PASS** (51 passed, 0 failed, 1 ignored):
  - `clipforge-capture`: 20 passed, 1 ignored (`live_gsr_replay_save_repeated_and_stop`, previously verified **PASS** with `-- --ignored` at `a1529cf`)
  - `clipforge-cloud-client`: 2 passed
  - `clipforge-common`: 3 passed
  - `clipforge-library`: 7 passed (integration)
  - `clipforge-media`: 13 passed (7 unit + 6 integration)
  - `clipforge-server`: 8 passed (5 unit + 3 integration)
- `cargo test -p clipforge-desktop`: **PASS** (14 passed, 0 failed)
- `cargo run -p clipforge-desktop -- --snap /tmp/clipforge-m1.png`: **PASS** (`gpu init 341.8 ms, first frame 225.31 ms`; `/tmp/clipforge-m1.png` 194,513 bytes)

### Unverified in Headless QA
- Interactive windowed Wayland pointer-drag timeline/trim-handle UX, context menu/multi-select mouse interaction, and live multi-frame playback in a Wayland window (`cargo run -p clipforge-desktop`).
- `live_gsr_replay_save_repeated_and_stop` remains `#[ignore]`d in default `cargo test`.
