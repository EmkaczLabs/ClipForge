# ClipForge Agent Guidelines

This file defines repository conventions, build/test commands, and agent ownership boundaries for resuming ClipForge development.

## Stack & Critical Constraints

- **Desktop UI:** `anomalyco/rune` (Git `rev = "dc8f7f7213cbe43af45212de6998d530d0945187"`, Rust `edition = "2024"`). Requires `[patch.crates-io]` in root `Cargo.toml`. Do **not** substitute Tauri, Electron, GTK, Qt, Slint, or egui.
- **Recording Engine:** `gpu-screen-recorder` (GSR) run as an independent managed child process (`crates/capture`). Never interfere with unrelated GSR instances (`SIGUSR1`/`SIGINT` only to our managed child PID).
- **Media Processing:** `ffmpeg` / `ffprobe` via `tokio::process::Command` arg arrays (`crates/media`). Never block the Rune UI thread.
- **Local Persistence:** SQLite via `rusqlite` with `bundled` feature (`crates/library`, `apps/server`). Parameterized queries only.
- **Server & Cloud:** Axum (`apps/server`), `reqwest` (`crates/cloud-client`), dark-first HTML5 player (`apps/web`).

## Workspace Ownership Boundaries

- `Cargo.toml`, `crates/common/`, `docs/`, `packaging/`: **Agent A (`arch`)**
- `apps/desktop/`: **Agent B (`ui`)**
- `crates/capture/`: **Agent C (`gsr`)**
- `crates/media/`, `crates/library/`: **Agent D (`media`)**
- `crates/cloud-client/`, `apps/server/`, `apps/web/`: **Agent E (`cloud`)**
- Verification & security review (read-only): **Agent F (`qa`)**

## Standard Verification Commands

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --exclude clipforge-desktop
cargo test -p clipforge-desktop
cargo run -p clipforge-desktop -- --snap /tmp/clipforge-m1.png
```

## Security & Engineering Rules

- Never construct shell commands from user-controlled input.
- Validate all file paths, storage keys, and subprocess arguments.
- Never commit secrets or tokens; `apps/server` uses PBKDF2-HMAC-SHA256 + SHA-256 token hashing.
- Never silently install system packages or run privileged commands without user approval.
