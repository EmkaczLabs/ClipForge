# ClipForge Progress Checkpoint

**Checkpoint Timestamp:** 2026-11-20T19:05:00+01:00  
**Latest Verified Commit:** `a30157e` (`main`)  
**Host Environment:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.

## Milestone Status Summary

| Milestone | Status | Notes |
|---|---|---|
| M1 — Foundation (Workspace & Rune UI) | Verified & passing | `clipforge-desktop` links and runs on CachyOS Wayland. Startup timing investigated: `setup_worker` `0.062 ms`, `WorkerCommand::Initialize` `13.99 ms`, headless `--snap` `gpu init ~234 ms` / `first frame ~219 ms`, windowed Wayland first `wl_surface.attach` ~2.0s (prior 21.7s attributed to cold-start Vulkan/fontique caches and debug link overhead). `scan_and_load_library` now emits cached SQLite clips immediately before probing new files. |
| M2 — Recording (`crates/capture`) | Verified with live GSR 6.1.7 | Fixed GSR 6.1.7 CLI args (`-bm auto` instead of `-bm cbr` with preset `-q`, `very_high` instead of `veryhigh`), deduplicated `ReplaySaved` events via `has_save_hook` filtering on `CLIPFORGE_SAVED:`, captured last stderr error line into `CaptureStatus::Error`, and added `Restart Replay Buffer` / `Clear Error` UI recovery. Live `live_gsr_replay_save_repeated_and_stop` test passed (2 distinct MP4s via `SIGUSR1`, clean `SIGINT` stop). Remaining gaps: `FocusedWindow` (`-w focused`) is X11-only, global hotkeys are X11-only, and `Av1` (on RTX 3070) / `Flac` emit GSR fallback warnings. |
| M3 — Library (`crates/library`, `crates/media`) | Verified & passing | `WorkerCommand::SaveReplay` now immediately indexes saved clips into SQLite (`index_single_clip` + `emit_library_loaded`). Fixed `FfmpegMediaProcessor::generate_thumbnail` past-EOF fallback (retries at `0.000` and returns `MediaFailed` if no frame is produced) and added canonical-path fallback in `SqliteClipStore::get_by_file_path`. Added SQLite v2 migration (`ClipOrigin`: `Recorded`, `Imported`, `Trimmed`, `Sample`) in `crates/library`; `apps/desktop` distinguishes sample clips via `"sample"` tag, `SAMPLE`/`REC` badges, library sample notice, and export/upload/play guards. |
| M4 — Editing (`crates/media`, `apps/desktop`) | Wired & verified | `FfmpegMediaProcessor::trim` tested (including multi-audio GSR-like MKV). Added `WorkerCommand::OpenInPlayer` (`xdg-open` on verified existing files) and `Open in Player` button + source path display in `editor.rs`. |
| M5 — Self-Hosted Sharing (`apps/server`, `crates/cloud-client`, `apps/web`) | Implemented & tested | 10 tests pass (auth, API keys, streaming multipart upload, HTTP `Range` seeking, public/private shares, `413` limit). End-to-end desktop-to-server live test pending. |
| M6 — Public Release (`packaging/`) | Scaffolding added | `Dockerfile`, `docker-compose.yml`, `PKGBUILD`, `deb/` templates created. Not yet built/validated end-to-end. |

## Priority Fixes & Findings (`19f0821`..`a30157e`)

1. **Real GSR capture on KDE Wayland + NVIDIA NVENC (`19f0821`):**
   - `crates/capture/src/args.rs`: Switched `-bm cbr` to `-bm auto` so `-q <preset>` is accepted by GSR 6.1.7; fixed `QualityPreset::VeryHigh` to emit `very_high`.
   - `crates/capture/src/backend.rs`: Added `has_save_hook` filtering so `-sc` mode emits `CaptureEvent::ReplaySaved` only on `CLIPFORGE_SAVED:<path>` (no duplicate events); crash monitor captures last `error` stderr line in `CaptureStatus::Error` / `CaptureEvent::Error`; `save_replay` distinguishes unexpected process exit from timeout.
   - Verified host discovery (`--list-monitors`: `DP-2|1920x1200`, `DP-3|1920x1080`; `--info`: `display_server|wayland`, `vendor|nvidia`, `gsr_version|6.1.7`) and live `live_gsr_replay_save_repeated_and_stop` test.
2. **Automatic SQLite indexing & media reliability (`334ea11`, `a30157e`):**
   - `apps/desktop/src/worker.rs`: `SaveReplay` `Ok(path)` now indexes `path` via `index_single_clip` + `emit_library_loaded`; `scan_and_load_library` emits cached SQLite clips before probing newly discovered files.
   - `crates/media`: `FfmpegMediaProcessor::generate_thumbnail` retries at `0.000` on past-EOF timestamps.
   - `crates/library`: `SqliteClipStore::get_by_file_path` falls back to canonicalized paths.
3. **Sample vs real recordings (`334ea11`, `a30157e`):**
   - `crates/library`: Added SQLite v2 schema (`clips.origin` + `idx_clips_origin`), `ClipOrigin` (`Recorded`, `Imported`, `Trimmed`, `Sample`), `ClipListOptions`, and `SqliteClipStore::{upsert_clip_with_origin, set_origin, get_origin, list_origins, list_clips_filtered}`.
   - `apps/desktop`: Seeded sample clips tagged `"sample"`, `SAMPLE`/`REC` badges across dashboard/library/editor, sample-data notice in library, export/upload/play guards for sample clips, and clearing of fake `shr_mirage_01` upload when real clips load.
4. **Video playback (`a30157e`):**
   - Added `WorkerCommand::OpenInPlayer` launching `xdg-open` only on verified existing files (`path.is_file()`).
5. **Rune startup timing (`a30157e`):**
   - Instrumented `setup_worker` (`0.062 ms`), `WorkerCommand::Initialize` (`13.99 ms`), `--snap` (`gpu init ~234 ms`, `first frame ~219 ms`), and windowed Wayland first attach (`~2.0s`).
6. **GSR failure & recovery (`19f0821`, `a30157e`):**
   - `StartReplayBuffer`/`StartRecording` failures emit `CaptureStatus::Error` with stderr context; `dashboard.rs` and `recording.rs` provide `Restart Replay Buffer` and `Clear Error` / `Dismiss Error` controls.

## Latest Verified Test Results (`a30157e`)

- `cargo fmt --all -- --check`: **PASS**
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** (0 crate warnings; 3 pre-existing workspace manifest warnings in `Cargo.toml`)
- `cargo test --workspace --exclude clipforge-desktop`: **PASS** (45 passed, 0 failed, 1 ignored):
  - `clipforge-capture`: 20 passed, 1 ignored (`live_gsr_replay_save_repeated_and_stop`, also verified **PASS** with `-- --ignored`)
  - `clipforge-cloud-client`: 2 passed
  - `clipforge-common`: 3 passed
  - `clipforge-library`: 4 passed (4 integration)
  - `clipforge-media`: 7 passed (4 unit + 3 integration)
  - `clipforge-server`: 8 passed (5 unit + 3 integration)
- `cargo test -p clipforge-desktop`: **PASS** (7 passed, 0 failed)
- `cargo run -p clipforge-desktop -- --snap /tmp/clipforge-m1.png`: **PASS** (`gpu init 234.3 ms, first frame 218.87 ms`)
