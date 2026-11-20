# ClipForge Progress Checkpoint

**Checkpoint Timestamp:** 2026-11-20T19:38:00+01:00  
**Latest Verified Commit:** `18cd8cb` (`main`)  
**Host Environment:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.

## Milestone Status Summary

| Milestone | Status | Notes |
|---|---|---|
| M1 — Foundation (Workspace & Rune UI) | Verified & passing | `clipforge-desktop` links and runs on CachyOS Wayland. Startup timing: `setup_worker` `0.062 ms`, `WorkerCommand::Initialize` `13.99 ms`, headless `--snap` `gpu init 415.6 ms` / `first frame 210.62 ms` (`/tmp/clipforge-m1.png`, 162,616 bytes), windowed Wayland first `wl_surface.attach` ~2.0s. `scan_and_load_library` emits cached SQLite clips immediately before probing new files. |
| M2 — Recording (`crates/capture`) | Verified with live GSR 6.1.7 | Fixed GSR 6.1.7 CLI args (`-bm auto`, `very_high`), deduplicated `ReplaySaved` via `has_save_hook` (`CLIPFORGE_SAVED:`), captured stderr errors into `CaptureStatus::Error`, and added `Restart Replay Buffer` / `Clear Error` UI recovery. Live `live_gsr_replay_save_repeated_and_stop` test passed (2 distinct MP4s via `SIGUSR1`, clean `SIGINT` stop). Remaining gaps: `FocusedWindow` (`-w focused`) and global hotkeys are X11-only; `Av1` (on RTX 3070) / `Flac` emit GSR fallback warnings. |
| M3 — Library (`crates/library`, `crates/media`, `apps/desktop`) | Verified & passing | `SaveReplay` indexes clips into SQLite immediately. `FfmpegMediaProcessor::generate_thumbnail` retries at `0.000` on past-EOF timestamps; `SqliteClipStore::get_by_file_path` has canonical-path fallback; SQLite v2 migration adds `ClipOrigin`. `views/library.rs` now supports click-to-select (`on_press`), double-click-to-Editor (`on_double_press`), active selection highlight, `lib-selected-detail` preview/metadata panel (`El::pixels_fit` thumbnail), and working Favorite / Trim & Edit / Open in Player / Upload actions synced with Editor. |
| M4 — Editing & Embedded Playback (`crates/media`, `apps/desktop`) | Implemented & tested (headless) | `crates/media/src/player.rs` (`VideoPlayerHandle`) decodes H.264/video off-UI-thread via `ffmpeg -f rawvideo -pix_fmt rgba pipe:1` (`kill_on_drop(true)`), delivering `DecodedFrame` RGBA8 buffers into `rune::Pixels::set` (`El::pixels_fit(..., SurfaceFit::Fit)`). `views/editor.rs` renders the embedded viewport, timeline thumbnails (`extract_timeline_thumbnails`), clickable/draggable playhead and trim-in/out handles (`El::on_drag`), `MM:SS.cc` timecode, trim-range playback, `Open in Player`, and `exporting` / `export_error` UI. Live interactive windowed Wayland pointer-drag UX and multi-frame window playback remain to be manually verified. |
| M5 — Self-Hosted Sharing (`apps/server`, `crates/cloud-client`, `apps/web`) | Implemented & tested | 10 tests pass (auth, API keys, streaming multipart upload, HTTP `Range` seeking, public/private shares, `413` limit). End-to-end desktop-to-server live test pending. |
| M6 — Public Release (`packaging/`) | Scaffolding added | `Dockerfile`, `docker-compose.yml`, `PKGBUILD`, `deb/` templates created. Not yet built/validated end-to-end. |

## Embedded Playback & Editor Architecture (`112ccc7`..`18cd8cb`)

1. **Off-UI-thread Decoder (`crates/media/src/player.rs`, `112ccc7`):**
   - `VideoPlayerHandle` (`open(path, trim_range, max_dim)`, `play`, `pause`, `seek`, `restart`, `set_trim_range`, `close`) spawns a `tokio` decoder task piping `ffmpeg -f rawvideo -pix_fmt rgba pipe:1` (`max_dim = Some(1024)` to stay within Rune's `1024x1024` image atlas fast-path).
   - Emits `PlayerEvent::{Metadata, Frame, StateChanged, Position, Error}`, paces playback to FPS, restricts playback/seeking to `[trim_start_ms, trim_end_ms]`, coalesces rapid `Seek` commands, emits immediate frames on `open`/`seek` while paused, and kills `ffmpeg` cleanly on clip switch/close/drop.
   - `extract_timeline_thumbnails(path, count, max_width, max_height)` extracts scrub-strip frames.
2. **Library Interaction & Worker Wiring (`apps/desktop/src/{main,state,worker,views/library}.rs`, `28d0093`):**
   - `views/library.rs`: `.on_press` (`select_clip`) and `.on_double_press` (`open_in_editor`) on Grid (`lib-card-{id}`) and List (`lib-row-{id}`) items; `lib-selected-detail` preview panel with `El::pixels_fit` thumbnail and Favorite / Trim & Edit / Open in Player / Upload controls.
   - `state.rs` & `worker.rs`: `EditorState` holds `video_pixels: rune::Pixels`, `has_video_frame`, `video_width`, `video_height`, `decoder_error`, `timeline_thumbs`, `exporting`, `export_error`; worker updates `rune::Pixels::set` off-UI-thread (auto-waking Rune) and skips `ffmpeg` for sample clips.
3. **Interactive Editor (`apps/desktop/src/views/editor.rs`, `5fe80d1`, `18cd8cb`):**
   - `video-viewport` renders `s.editor.video_pixels` via `El::pixels_fit(..., SurfaceFit::Fit)` with `.clip_rounded(radius::MD)` and distinct sample/loading/`decoder_error` states.
   - `timeline-track` renders `timeline_thumbs`, `timeline-selection`, `timeline-playhead`, `El::on_drag` scrubbing, and draggable `trim-in-handle`/`trim-out-handle` controls, plus `exporting` and inline `export_error` feedback.

## Latest Verified Test Results (`18cd8cb`)

- `cargo fmt --all -- --check`: **PASS**
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** (0 crate warnings; 3 pre-existing workspace manifest warnings in `Cargo.toml`)
- `cargo test --workspace --exclude clipforge-desktop`: **PASS** (48 passed, 0 failed, 1 ignored):
  - `clipforge-capture`: 20 passed, 1 ignored (`live_gsr_replay_save_repeated_and_stop`, previously verified **PASS** with `-- --ignored` at `a1529cf`)
  - `clipforge-cloud-client`: 2 passed
  - `clipforge-common`: 3 passed
  - `clipforge-library`: 4 passed (4 integration)
  - `clipforge-media`: 13 passed (7 unit + 6 integration, including real H.264 MP4 open/seek/play/trim/switch/error/thumbnail tests)
  - `clipforge-server`: 8 passed (5 unit + 3 integration)
- `cargo test -p clipforge-desktop`: **PASS** (9 passed, 0 failed, including Library click/double-click/actions and Editor playback/trim helper tests)
- `cargo run -p clipforge-desktop -- --snap /tmp/clipforge-m1.png`: **PASS** (`gpu init 415.6 ms, first frame 210.62 ms`; `/tmp/clipforge-m1.png` 162,616 bytes)

### Unverified in Headless QA
- Interactive windowed Wayland pointer-drag timeline/trim-handle UX and live multi-frame playback in a Wayland window (`cargo run -p clipforge-desktop`).
- `live_gsr_replay_save_repeated_and_stop` remains `#[ignore]`d in default `cargo test`.
