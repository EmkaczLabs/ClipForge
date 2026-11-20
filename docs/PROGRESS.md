# ClipForge Progress Checkpoint

**Checkpoint Timestamp:** 2026-11-20T20:14:00+01:00  
**Latest Verified Commit:** `40311c8` (`main`)  
**Host Environment:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.

## Milestone Status Summary

| Milestone | Status | Notes |
|---|---|---|
| M1 — Foundation (Workspace & Rune UI) | Verified & passing | `clipforge-desktop` links and runs on CachyOS Wayland. Headless `--snap`: `gpu init 323.2 ms` / `first frame 220.00 ms` (`/tmp/clipforge-m1.png`, 163,587 bytes); windowed Wayland first `wl_surface.attach` ~2.0s. |
| M2 — Recording (`crates/capture`) | Verified with live GSR 6.1.7 | Fixed GSR 6.1.7 CLI args (`-bm auto`, `very_high`), deduplicated `ReplaySaved` (`CLIPFORGE_SAVED:`), stderr capture into `CaptureStatus::Error`, and `Restart Replay Buffer` / `Clear Error` UI. Live `live_gsr_replay_save_repeated_and_stop` passed (`a1529cf`). Remaining gaps: `FocusedWindow` and global hotkeys are X11-only; `Av1` (on RTX 3070) / `Flac` emit GSR fallback warnings. |
| M3 — Library & Management (`crates/library`, `crates/media`, `apps/desktop`) | Verified & passing | SQLite v3 trash/restore (`trashed_at`, `original_path`, `.clipforge_trash`), managed-root boundary validation, `Sample`/`Imported` disk protection, `rename_clip`, `play_in_editor` vs `open_in_editor`, Ctrl/Shift multi-select + bulk actions, inline confirmation/rename, sort/filter, floating context menu, and automatic `ClipOrigin::Trimmed` indexing on export. |
| M4 — Native Video Player & Editor (`crates/media`, `apps/desktop`) | Implemented & tested (headless) | `crates/media` (`8abb686`) decodes video via `ffmpeg` rawvideo RGBA8 pipe (benchmarked 609.8 fps `1080p60 -> 1024x576`, 286.0 fps native `1920x1080`) with bounded `mpsc` queue, `cpal 0.15` (`pipewire-alsa`) audio output, `samples_played` master clock A/V sync, monotonic wall-clock fallback, `set_audio_track(Option<usize>)`, and `ExportCancelToken` + `trim_with_progress`. `apps/desktop` (`5e3f3eb`, `40311c8`) adds `play_in_editor`, mute/volume slider, timeline zoom (`1x..8x`) + `scroll_x`, keyboard shortcuts (`Space`, `Left`/`Right`), and export progress bar + `Cancel Export`. Manual windowed Wayland auditioning/drag UX remains to be verified interactively. |
| M5 — Self-Hosted Sharing (`apps/server`, `crates/cloud-client`, `apps/web`) | Implemented & tested | 10 tests pass (auth, API keys, streaming multipart upload, HTTP `Range` seeking, public/private shares, `413` limit). End-to-end desktop-to-server live test pending. |
| M6 — Public Release (`packaging/`) | Scaffolding added | `Dockerfile`, `docker-compose.yml`, `PKGBUILD`, `deb/` templates created. Not yet built/validated end-to-end. |

## Native Video Player & Editor Architecture (`8abb686`, `5e3f3eb`, `40311c8`)

1. **Decoder, Audio & A/V Sync (`crates/media/src/{player,audio,processor}.rs`, `8abb686`):**
   - **Throughput & Bounded Buffering:** `ffmpeg` rawvideo RGBA8 pipe benchmarked on this host at **609.8 fps** (`1080p60 -> 1024x576`) and **286.0 fps** (native `1920x1080`). `VideoPlayerHandle::spawn()` uses a bounded `mpsc::Receiver<PlayerEvent>` (capacity 16, reserving 4 slots for control events and dropping `Frame` events under backpressure) plus A/V drift frame-dropping when video lags `>2` frames behind the master clock.
   - **Audio Output (`cpal 0.15` via `pipewire-alsa`):** `ffmpeg` `f32le` decode pipe into a bounded 1s PCM ring buffer (`MAX_BUFFER_SECONDS = 1`) with sample-accurate `samples_played` master clock, `set_volume(f32)` / `set_muted(bool)` / `set_audio_track(Option<usize>)`, `PlayerEvent::AudioStatus { has_audio, error }`, and monotonic wall-clock fallback for no-audio clips or headless/device failure.
   - **Export Progress & Cancellation:** `ExportCancelToken` + `FfmpegMediaProcessor::trim_with_progress` parses `-progress pipe:1` (`out_time_us`/`out_time_ms`), kills the child process and cleans temp files on cancel, and validates output via non-empty check + `ffprobe`.
2. **Desktop State, Worker & Editor UI (`apps/desktop`, `5e3f3eb`, `40311c8`):**
   - `state.rs` / `worker.rs` / `views/library.rs` / `main.rs` (`5e3f3eb`): Added `play_in_editor(id)` (opens Editor and auto-plays) alongside `open_in_editor(id)`, `set_volume`, `toggle_mute`, `set_timeline_zoom`, `seek_relative_ms`, `cancel_export`, `AppSpec::on_key` (`Space`, `Left`/`Right`), safe player stop on trash/delete, and automatic SQLite indexing of exported clips as `ClipOrigin::Trimmed`.
   - `views/editor.rs` (`40311c8`): Added `audio-controls` (`mute-toggle` + `El::on_drag` `volume-slider` + `Audio Off`/`No Audio` badges), `timeline-zoom-controls` (`-`, `1x`, `+` scaling `track_w = 640.0 * zoom` in `1.0..=8.0` inside `El::scroll_x()`), `El::focusable(true)` + `El::on_key` shortcuts, and `export-progress-box` + `Cancel Export` button.

## Latest Verified Test Results (`40311c8`)

- `cargo fmt --all -- --check`: **PASS**
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** (0 crate warnings; 3 pre-existing workspace manifest warnings in `Cargo.toml`)
- `cargo test --workspace`: **PASS** (71 passed, 0 failed, 1 ignored):
  - `clipforge-capture`: 20 passed, 1 ignored (`live_gsr_replay_save_repeated_and_stop`, previously verified **PASS** with `-- --ignored` at `a1529cf`)
  - `clipforge-cloud-client`: 2 passed
  - `clipforge-common`: 3 passed
  - `clipforge-desktop`: 15 passed
  - `clipforge-library`: 7 passed (integration)
  - `clipforge-media`: 16 passed (7 unit + 9 integration, including real-MP4 A/V sync, no-audio fallback, volume/mute, clip switching, and export progress/cancel)
  - `clipforge-server`: 8 passed (5 unit + 3 integration)
- `cargo run -p clipforge-desktop -- --snap /tmp/clipforge-m1.png`: **PASS** (`gpu init 323.2 ms, first frame 220.00 ms`; `/tmp/clipforge-m1.png` 163,587 bytes)

### Manual GUI Acceptance Steps Remaining
- Launch `cargo run -p clipforge-desktop` in the KDE Wayland session and verify:
  1. PipeWire/ALSA audio playback auditioning, mute toggle, and `volume-slider` drag.
  2. Live pointer-drag timeline scrubbing, trim-in/trim-out handle dragging, and `1x..8x` timeline zoom scrolling.
  3. Library click/double-click/`Play`, context menu, multi-select, trash/restore/delete, and live FFmpeg export progress/cancellation.
  4. Optional live GSR capture test: `cargo test -p clipforge-capture -- --ignored live_gsr_replay_save_repeated_and_stop`.
