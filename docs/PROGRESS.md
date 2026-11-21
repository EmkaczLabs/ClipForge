# ClipForge Progress Checkpoint

**Checkpoint Timestamp:** 2026-11-21T03:50:00+01:00  
**Latest Verified Commit (`ClipForge`):** `7ef7763` + `apps/server` fix (`main`)  
**Latest Verified Commit (`ClipForge-Server`):** `9216eb2` (`main` at `/home/emkacz/Projects/ClipForge-Server`)  
**Proxmox Deployment:** LXC `112` (`192.168.1.112:3000`, `running`, `healthy`; **needs rebuild/redeploy from `ClipForge-Server` `9216eb2`** to pick up server-side `DELETE` & visibility-sync fixes)  
**Host Environment:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.

## 1. Current Project Architecture & Implementation Status

| Component | Status | Notes |
|---|---|---|
| Desktop Foundation & Obsidian Forge UI (`ClipForge/apps/desktop`) | Verified & passing (`a980e92`, `7ef7763`) | 5-screen `NavSection` (`Library` default, `Editor`, `Recording`, `Uploads`, `Settings`), 7-tier Obsidian Forge palette, SVG icon & primitive library, 48px topbar + 64px rail + 28px status bar. Fixed text input `.on_edit` focus/typing/paste/Tab/`.secure()` masking (`a980e92`). |
| Capture, Audio Routing & Hotkeys (`ClipForge/crates/capture`) | Verified (live GSR + `ffprobe`) | GSR 6.1.7 supervision (`-bm auto`, `very_high`, `CLIPFORGE_SAVED:` deduplication), deterministic `Auto`/`T1`–`T4` audio routing (`\|` mixing into `0:a:0`, `t2:`–`t4:` separate tracks), and XDG `GlobalShortcuts` (`zbus`) + X11 fallback + `AppSpec::on_key` (`F9`/`F10`/`F11`). |
| Library (`ClipForge/crates/library`, `views/library.rs`) | Verified & passing | SQLite v3 (`ClipOrigin`, `trashed_at`, `original_path`, `.clipforge_trash`), managed-root containment checks, `Sample`/`Imported` disk protection, multi-select, inline confirm/rename, context menu, and 320px Inspector. |
| Media Player, Audio & Mic Test (`ClipForge/crates/media`, `views/editor.rs`, `views/recording.rs`) | Verified (automated + PipeWire unit/integration) | Off-UI-thread `ffmpeg` RGBA8 pipe + `cpal` PipeWire/ALSA audio + `AutoMix` multi-track playback, `ExportCancelToken` + `trim_with_progress`, and standalone `MicTestHandle` (`pactl`+`parec`+`cpal`, real dBFS, 3s record/playback, `kill_on_drop`). |
| Desktop Cloud, Lifecycle & Token Hardening (`ClipForge/crates/cloud-client`, `apps/desktop`, `apps/server`) | Verified (`1afbd80`, `7ef7763` + `apps/server` fix) | `/api/v1` `HttpCloudClient` (resumable multipart uploads, shares, server info/quota, redacted `Debug`), strict `secret-tool` token storage (`1afbd80`), and P0 Remote Clip Lifecycle fixes (`7ef7763`: `PATCH/DELETE /api/v1/clips/{clip_id}`, no optimistic removal/mutation on failure, refreshed unshared clips retain Delete/Visibility/Create Share). Legacy `apps/server` updated with `PATCH /api/v1/clips/{id}`. |
| Standalone `ClipForge-Server` (`ClipForge-Server`, `9216eb2`) | Verified (`9216eb2`); LXC `112` rebuild pending | 5-crate workspace + `web/index.html` + `Dockerfile` + `compose.yaml`. `9216eb2` fixes `PATCH /api/v1/clips/{id}` syncing `shares.visibility` (plus `PATCH /api/v1/shares/{token}/visibility`) and `DELETE /api/v1/clips/{id}` deleting storage objects *before* SQLite `clips` rows so storage errors preserve DB state for retry. |

## 2. Key Architectural Decisions & Findings

- **P0 Remote Clip Lifecycle Fixes (`9216eb2`, `7ef7763`):**
  - **Visibility `404`:** Client now sends `PATCH /api/v1/clips/{clip_id}` instead of passing `share_id` (share token), and server atomically syncs `clips.visibility` and active `shares.visibility`.
  - **Remote Delete:** Client calls `DELETE /api/v1/clips/{clip_id}` directly (no `delete_share` fallback) and removes UI items only after confirmed server `Ok`. Server deletes all `clip_assets` from `StorageBackend` *before* deleting the SQLite `clips` row.
  - **Refreshed Unshared Clips:** `views/uploads.rs` keys Visibility, Delete from Server, and Create Share Link on `clip_id` rather than `share_id`.
- **Text Input & Focus (`a980e92`):** `rune::text_input` requires `.on_edit(...)` for `focus_input` to focus on click; wired `.on_edit`, `.single_line().placeholder(...)`, `.focus_within_ring(...)`, and `.secure()` masking.
- **LXC 112 Deployment:** Currently running `8b6326d` in local-storage test mode; **needs a container rebuild/redeploy from `ClipForge-Server` `9216eb2`** (once approved) to pick up server-side `DELETE` and visibility-sync fixes.

## 3. Verified Build & Test Results

- **`/home/emkacz/Projects/ClipForge` (with `apps/server` `PATCH /api/v1/clips/{id}` fix):**
  - `cargo fmt --all -- --check`: **PASS**
  - `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** (0 crate warnings; 3 pre-existing manifest warnings in `Cargo.toml`)
  - `XDG_CONFIG_HOME=/tmp/clipforge-test-cfg cargo test --workspace`: **PASS** (108 passed, 0 failed, 3 ignored: `clipforge-capture` 31+3 ignored, `clipforge-cloud-client` 6, `clipforge-common` 3, `clipforge-desktop` 29, `clipforge-library` 7, `clipforge-media` 24, `clipforge-server` 8).
- **`/home/emkacz/Projects/ClipForge-Server` (`9216eb2`):**
  - `cargo fmt --all -- --check`: **PASS**
  - `cargo clippy --workspace --all-targets -- -D warnings`: **PASS**
  - `cargo test --workspace`: **PASS** (24 passed, 0 failed)
  - **Live Local E2E (`#197`):** **PASS** (upload, share, `public <-> private` sync, share revoke keeping clip with `share_id: None`, `DELETE /api/v1/clips/{id}` purging storage file + `404`/`410`, repeated `DELETE` -> `404`).
