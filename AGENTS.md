# ClipForge Agent Guidelines

This file defines repository conventions, build/test commands, and agent ownership boundaries for resuming ClipForge development.

## Repositories & Architecture

- **Desktop Client (`/home/emkacz/Projects/ClipForge`):**
  - `apps/desktop`: Rune (`anomalyco/rune`, Git `rev = "dc8f7f7213cbe43af45212de6998d530d0945187"`, Rust `edition = "2024"`) 5-screen Obsidian Forge UI (`Library`, `Editor`, `Recording`, `Uploads`, `Settings`).
  - `crates/common`: Shared domain models, `AppConfig` (with `HotkeyBindings`), `CaptureConfig`, traits, and errors.
  - `crates/capture`: `gpu-screen-recorder` (GSR) supervision (`SIGUSR1`/`SIGINT` only to our managed child PID), deterministic `Auto`/`T1`–`T4` audio routing (`crates/capture/src/audio.rs`), and XDG `GlobalShortcuts` (`zbus`) + X11 fallback (`crates/capture/src/hotkey.rs`).
  - `crates/media`: `ffmpeg`/`ffprobe` processing, `VideoPlayerHandle` (off-UI-thread RGBA8 pipe + `cpal` PipeWire/ALSA audio + `AutoMix` multi-track playback), `ExportCancelToken` + `trim_with_progress`, and `MicTestHandle` (`pactl`+`parec`+`cpal`).
  - `crates/library`: SQLite (`rusqlite` bundled) v3 schema (`ClipOrigin`, `trashed_at`, `original_path`, `.clipforge_trash`) + `DirectoryScanner`.
  - `crates/cloud-client`: `/api/v1` `HttpCloudClient` (resumable multipart uploads, shares, server info/quota, redacted `Debug`).
  - `apps/site`: Official Next.js App Router product website (`https://clipforge.emkacz.dev`) with RustFS release manifest/download Route Handlers, MDX docs, and Vitest/Playwright tests.
- **Standalone Server (`/home/emkacz/Projects/ClipForge-Server`):**
  - Modular 5-crate workspace (`clipforge-api` OpenAPI 3.1.0, `clipforge-db`, `clipforge-auth` Argon2id + SHA-256 tokens, `clipforge-storage` Local + RustFS S3 SigV4, `clipforge-server` Axum `/api/v1` + CLI) + `web/index.html` + `Dockerfile` + `compose.yaml`.
  - Zero local path dependencies to `/home/emkacz/Projects/ClipForge`.

## Stack & Critical Constraints

- **Desktop UI:** `anomalyco/rune` (`rev = "dc8f7f7213cbe43af45212de6998d530d0945187"`, Rust `edition = "2024"`). Requires `[patch.crates-io]` in root `Cargo.toml`. Do **not** substitute Tauri, Electron, GTK, Qt, Slint, or egui.
- **Recording Engine:** `gpu-screen-recorder` run as an independent managed child process (`crates/capture`).
- **Media Processing:** `ffmpeg` / `ffprobe` via `tokio::process::Command` arg arrays (`crates/media`). Never block the Rune UI thread.
- **Local Persistence:** SQLite via `rusqlite` with `bundled` feature. Parameterized queries only.
- **Token Storage:** Desktop tokens use `secret-tool` (`org.freedesktop.secrets`); never write plaintext tokens to `config.json` or fallback files.

## Workspace Ownership Boundaries

- `Cargo.toml`, `crates/common/`, `docs/`, `packaging/`, `AGENTS.md`: **Agent A (`arch`)**
- `apps/desktop/`: **Agent B (`ui`)**
- `crates/capture/`: **Agent C (`gsr`)**
- `crates/media/`, `crates/library/`: **Agent D (`media`)**
- `crates/cloud-client/`, `ClipForge-Server`: **Agent E (`cloud`)**
- Verification & security review (read-only): **Agent F (`qa`)**

## Standard Verification Commands

```bash
# In /home/emkacz/Projects/ClipForge
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p clipforge-desktop -- --scene library --snap /tmp/clipforge-m1.png

# In /home/emkacz/Projects/ClipForge-Server
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Security & Engineering Rules

- Never construct shell commands from user-controlled input.
- Validate all file paths, storage keys, and subprocess arguments.
- Never commit secrets, `.env` files, or API tokens; never log bearer tokens or S3 secrets.
- Never silently install system packages or run privileged commands without user approval.
