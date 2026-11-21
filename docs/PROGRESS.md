# ClipForge Progress Checkpoint

**Checkpoint Timestamp:** 2026-11-21T03:04:00+01:00  
**Latest Verified Commit (`ClipForge`):** `1afbd80` (`main`)  
**Latest Verified Commit (`ClipForge-Server`):** `8b6326d` (`main` at `/home/emkacz/Projects/ClipForge-Server`)  
**Proxmox Deployment:** LXC `112` (`192.168.1.112:3000`, `running`, `healthy`, `/ready` returning `db_ok:true, storage_ok:true`)  
**Host Environment:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.

## 1. Current Project Architecture & Implementation Status

| Component | Status | Notes |
|---|---|---|
| Desktop Foundation & Obsidian Forge UI (`ClipForge/apps/desktop`) | Verified & passing | 5-screen `NavSection` (`Library` default, `Editor`, `Recording`, `Uploads`, `Settings`), 7-tier Obsidian Forge palette (`theme.rs`), SVG icon & primitive library (`components.rs`), 48px topbar + 64px rail + 28px docked status bar (`views/mod.rs`), all 5 `--scene <name> --snap` renders pass. |
| Capture, Audio Routing & Hotkeys (`ClipForge/crates/capture`) | Verified (live GSR + `ffprobe`) | GSR 6.1.7 supervision (`-bm auto`, `very_high`, `CLIPFORGE_SAVED:` deduplication), deterministic `Auto`/`T1`–`T4` audio routing (`\|` mixing into `0:a:0`, `t2:`–`t4:` separate tracks), and XDG `GlobalShortcuts` (`zbus`) + X11 fallback + `AppSpec::on_key` (`F9`/`F10`/`F11`). |
| Library (`ClipForge/crates/library`, `views/library.rs`) | Verified & passing | SQLite v3 (`ClipOrigin`, `trashed_at`, `original_path`, `.clipforge_trash`), managed-root containment checks, `Sample`/`Imported` disk protection, multi-select, inline confirm/rename, context menu, and 320px Inspector. |
| Media Player, Audio & Mic Test (`ClipForge/crates/media`, `views/editor.rs`, `views/recording.rs`) | Verified (automated + PipeWire unit/integration) | Off-UI-thread `ffmpeg` RGBA8 pipe + `cpal` PipeWire/ALSA audio + `AutoMix` multi-track playback, `ExportCancelToken` + `trim_with_progress`, and standalone `MicTestHandle` (`pactl`+`parec`+`cpal`, real dBFS, 3s record/playback, `kill_on_drop`). |
| Desktop Cloud & Token Hardening (`ClipForge/crates/cloud-client`, `apps/desktop/src/config.rs`) | Verified (`1afbd80`) | `/api/v1` `HttpCloudClient` (resumable multipart uploads, shares, server info/quota, redacted `Debug`) and strict `secret-tool` (`org.freedesktop.secrets`) token storage with no plaintext `credentials.json` fallback (`1afbd80`). |
| Standalone `ClipForge-Server` (`ClipForge-Server`, `8b6326d`) | Deployed & verified on LXC `112` | 5-crate workspace (`clipforge-api` OpenAPI 3.1.0, `clipforge-db`, `clipforge-auth` Argon2id + SHA-256 tokens, `clipforge-storage` Local + RustFS S3 SigV4, `clipforge-server`), `web/index.html` Range player, `/ready` Docker healthcheck, Compose log rotation (`8b6326d`), and Proxmox backup job (`112` in `/etc/pve/jobs.cfg`). |

## 2. Key Architectural Decisions & Findings

- **Audio Routing Root Cause:** GSR 6.1.7 writes separate audio streams for repeated `-a` flags; `Auto` and `T1` now join sources with `|` (`-a "default_output|default_input"`) into `0:a:0`, while `crates/media` defaults to `AudioTrackMode::AutoMix` (`amix` for multi-track files).
- **Standalone Server Extraction:** `ClipForge-Server` lives in `/home/emkacz/Projects/ClipForge-Server` with zero path dependencies to `ClipForge`, enabling independent container builds on Proxmox LXC `112`.
- **Token Security:** Desktop `api_token` is never written to `config.json` and no longer falls back to plaintext `credentials.json` (`1afbd80`); `secret-tool` (`org.freedesktop.secrets`) is required.
- **LXC 112 Deployment Mode:** Currently deployed in hardened **local-storage test mode** (`http://192.168.1.112:3000`, `CLIPFORGE_STORAGE_BACKEND=local`). RustFS (`111`) and Cloudflare Tunnel (`102`) / NPM (`108`) remain untouched pending separate approval.

## 3. Verified Build & Test Results

- **`/home/emkacz/Projects/ClipForge` (`1afbd80`):**
  - `cargo fmt --all -- --check`: **PASS**
  - `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** (0 crate warnings; 3 pre-existing manifest warnings in `Cargo.toml`)
  - `cargo test --workspace`: **PASS** (102 passed, 0 failed, 3 ignored: `live_gsr_replay_save_repeated_and_stop`, `live_wayland_portal`, `live_audio_routing`)
  - 5-screen `--snap` renders (`library`, `editor`, `recording`, `uploads`, `settings`) verified with zero Rune a11y/layout warnings; `live_audio_routing` 1/1 PASS against real GSR 6.1.7 + `ffprobe`.
- **`/home/emkacz/Projects/ClipForge-Server` (`8b6326d`):**
  - `cargo fmt --all -- --check`: **PASS**
  - `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** (0 crate warnings; 1 unused `workspace.package.repository` warning)
  - `cargo test --workspace`: **PASS** (22 passed, 0 failed)
  - Read-only `S3_ENDPOINT=https://s3.emkacz.dev S3_BUCKET=clipforge CLIPFORGE_STORAGE_BACKEND=s3 ./target/release/clipforge-server storage check`: `config_validation`, `dns_resolution`, `tls_connectivity` **PASS** (`bucket_auth` & `write_probe` skipped without credentials).
- **Live LXC `112` E2E & Restart Persistence (`http://192.168.1.112:3000`):**
  - **PASS**: `/health` & `/ready` (`200 OK`), unauthenticated `401`, register/login/API token, 7,253 B MP4 chunked upload, cross-user isolation (`403`/`404`), `docker compose restart clipforge-server` persistence, `Range: bytes=0-1023` -> `206`, `GET /c/{share}` (`200`), and cleanup (`DELETE` -> `410`).
