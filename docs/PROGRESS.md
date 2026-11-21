# ClipForge Progress Checkpoint

**Checkpoint Timestamp:** 2026-11-21T01:00:00+01:00  
**Latest Verified Commit (`ClipForge`):** `786d61a` (`main`)  
**Latest Verified Commit (`ClipForge-Server`):** `b4d32e0` (`main` at `/home/emkacz/Projects/ClipForge-Server`)  
**Host Environment:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.

## Milestone Status Summary

| Milestone | Status | Notes |
|---|---|---|
| M1 — Foundation & Obsidian Forge UI (`97844c6..7b3da9a`) | Verified & passing | 5-screen `NavSection` (`Library` default, `Editor`, `Recording`, `Uploads`, `Settings`), Obsidian Forge palette (`theme.rs`), SVG icon & primitive catalog (`components.rs`), 48px topbar + 64px rail + 28px status bar (`views/mod.rs`), all 5 `--scene <name> --snap` renders pass. |
| M2 — Recording Studio, Audio Routing & Hotkeys (`8ff12e8`, `636fce3`, `1592c5a`) | Verified (live GSR + `ffprobe`) | Deterministic `Auto` vs `T1`–`T4` GSR audio routing (`crates/capture/src/audio.rs`): `Auto`/`T1` mix via `\|` into `0:a:0`; `t2:`–`t4:` emit separate `-a` tracks. Live `live_audio_routing` 1/1 PASS across all 5 cases. Global hotkeys use XDG `GlobalShortcuts` (`zbus`) + X11 fallback + `AppSpec::on_key`. |
| M3 — Clip Library (`crates/library`, `views/library.rs`) | Verified & passing | SQLite v3 trash/restore (`trashed_at`, `original_path`, `.clipforge_trash`), managed-root boundary validation, `Sample`/`Imported` disk protection, `rename_clip`, game filter, Ctrl/Shift multi-select + bulk actions, inline confirm/rename, floating context menu, and 320px Inspector. |
| M4 — Video Editor, Multi-Track Audio & Mic Test (`3674bce`, `636fce3`) | Verified (automated + PipeWire unit/integration) | `AudioTrackMode::AutoMix` (`amix` for multi-track files) + `MicTestHandle` (`pactl` + `parec` + `cpal`, real dBFS metering, 3s sample record/playback, no silent device fallback, `kill_on_drop(true)` cleanup). |
| M5 — Standalone `ClipForge-Server` & `/api/v1` Desktop Cloud (`786d61a`, `b4d32e0`) | Verified (unit/integration + live E2E) | Extracted standalone `/home/emkacz/Projects/ClipForge-Server` (`b4d32e0`, 5 crates: `clipforge-api`, `clipforge-db`, `clipforge-auth`, `clipforge-storage`, `clipforge-server`, `migrations/`, `web/index.html`, `Dockerfile`, `compose.yaml`). Supports Argon2id + SHA-256 API tokens, resumable multipart uploads, `LocalStorage` + RustFS `S3Storage` (SigV4 + Range streaming), and `/c/{share_id}` HTML5 player. Desktop (`786d61a`) upgraded to `/api/v1` with `secret-tool` + `0600` `credentials.json` token storage (`api_token` redacted from `config.json`). Live E2E against `clipforge-server serve` passed. |
| M6 — Release & Packaging (`ClipForge/packaging/`, `ClipForge-Server/{Dockerfile,compose.yaml,.github}`) | Scaffolding ready; publish pending | `ClipForge-Server` includes `Dockerfile`, `compose.yaml`, `.env.example`, `.github/workflows/ci.yml`, `README.md`, `SECURITY.md`, `docs/{deployment,backup-restore}.md`. `ClipForge` `packaging/` (`PKGBUILD`, `deb/`) remains to be built/validated; neither repo has been pushed. |

## Standalone `ClipForge-Server` & `/api/v1` Desktop Integration (`786d61a`, `b4d32e0`)

1. **Standalone Server Repository (`/home/emkacz/Projects/ClipForge-Server`, `b4d32e0`):**
   - Zero local path dependencies to `/home/emkacz/Projects/ClipForge`.
   - **`clipforge-api`**: `/api/v1` DTOs + OpenAPI 3.1.0 spec (`openapi.yaml`).
   - **`clipforge-db` & `clipforge-auth`**: SQLite migrations (`0001_initial.sql`), Argon2id passwords, SHA-256 hashed personal API tokens, user/admin roles, quota enforcement, and cross-user isolation.
   - **`clipforge-storage`**: `StorageBackend` trait, `LocalStorage` (atomic writes, Range reads), `S3Storage` (RustFS-compatible AWS SigV4 via `ring`/`hex`/`reqwest`, Range reads, multipart upload/abort), and `storage check` CLI.
   - **`clipforge-server` & `web/index.html`**: Axum `/api/v1` router, HTTP `206` byte-range streaming, `/c/{share_id}` dark HTML5 player, and `serve` / `storage check` / `admin` CLI.
   - Read-only `S3_ENDPOINT=https://s3.emkacz.dev S3_BUCKET=clipforge CLIPFORGE_STORAGE_BACKEND=s3 ./target/release/clipforge-server storage check` passed `config_validation`, `dns_resolution` (`s3.emkacz.dev:443`), and `tls_connectivity` (`HTTP 501 Not Implemented`), skipping `bucket_auth` and `write_probe` without credentials.
2. **Desktop `/api/v1` Cloud Client & Secure Token Storage (`ClipForge` `786d61a`):**
   - `crates/cloud-client`: Added `/api/v1/server/info`, `/api/v1/quota`, `/api/v1/auth/{me,tokens}`, resumable multipart uploads (`/api/v1/uploads/...`), `PATCH /api/v1/clips/{id}`, and `POST /api/v1/clips/{id}/shares`.
   - `apps/desktop/src/{config,state,worker,views/uploads}.rs`: Token stored via `secret-tool` (libsecret) with `0600` `credentials.json` fallback and `api_token` redacted from `config.json` (`config_roundtrip_temp_file_redacts_api_token`); `views/uploads.rs` renders `/api/v1` server info, quota bar, and upload/share controls.

## Latest Verified Test Results (`786d61a` & `b4d32e0`)

- **`/home/emkacz/Projects/ClipForge` (`786d61a`):**
  - `cargo fmt --all -- --check`: **PASS**
  - `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** (0 crate warnings; 3 pre-existing workspace manifest warnings in `Cargo.toml`)
  - `cargo test --workspace`: **PASS** (102 passed, 0 failed, 3 ignored):
    - `clipforge-capture`: 31 passed, 3 ignored (`live_gsr_replay_save_repeated_and_stop`, `live_wayland_portal`, `live_audio_routing`)
    - `clipforge-cloud-client`: 4 passed
    - `clipforge-common`: 3 passed
    - `clipforge-desktop`: 24 passed
    - `clipforge-library`: 7 passed (integration)
    - `clipforge-media`: 24 passed (13 unit + 11 integration)
    - `clipforge-server`: 8 passed (5 unit + 3 integration)
  - `cargo run -p clipforge-desktop -- --scene uploads --snap /tmp/clipforge-uploads-v1.png`: **PASS** (255,523 B; visually inspected, zero Rune a11y warnings).
- **`/home/emkacz/Projects/ClipForge-Server` (`b4d32e0` / `a77bc5c`):**
  - `cargo fmt --all -- --check`: **PASS**
  - `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** (0 crate warnings; 1 unused `workspace.package.repository` warning)
  - `cargo test --workspace`: **PASS** (22 passed, 0 failed — `clipforge-api`: 2, `clipforge-auth`: 4, `clipforge-db`: 4, `clipforge-server`: 1+3, `clipforge-storage`: 8)
  - `cargo build --release`: **PASS**
- **Live Cross-Repo E2E (`clipforge-server serve` on ephemeral port `44627`, temp SQLite + local storage):**
  - **PASS**: User A register/login, `GET /api/v1/server/{info,quota}`, streaming MP4 upload (`7,253` B) + `complete`, `GET /api/v1/clips`, User B denied on User A's private clip, `PATCH /api/v1/clips/{id}` public, `POST .../shares`, anonymous `Range: bytes=0-1023` -> `206` (1,024 B), `GET /c/{share}` HTML5 `<video>` (`200`), `DELETE /api/v1/shares/{token}` -> `410 Gone`, clean shutdown.

### Remaining Manual & Publish Steps
- Interactive windowed auditioning in KDE Wayland (`Test Microphone` with live voice + `Play Sample`, KDE `GlobalShortcuts` portal dialog approval + unfocused `F9`/`F10`/`F11`, pointer-drag editor UX).
- Run authenticated `clipforge-server storage check --write-probe` against `https://s3.emkacz.dev` once S3 credentials are configured in the server runtime environment.
- Push `ClipForge` and `ClipForge-Server` `main` branches when ready to publish.
