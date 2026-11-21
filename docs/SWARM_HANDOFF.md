# ClipForge Swarm Handoff

## Current Environment & Baseline

- **Host:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`.
- **Verified Commits:**
  - `ClipForge` (`/home/emkacz/Projects/ClipForge`): `786d61a` on `main` (102 passed, 0 failed, 3 ignored).
  - `ClipForge-Server` (`/home/emkacz/Projects/ClipForge-Server`): `b4d32e0` on `main` (22 passed, 0 failed; release build + read-only `storage check` against `https://s3.emkacz.dev` + live cross-repo E2E verified).

## Swarm State

| Member | Role | Ownership | Latest Verified Work |
|---|---|---|---|
| `arch` | Architecture & Docs | `ClipForge/{Cargo.toml,crates/common,docs,apps/desktop/src/{state,worker}.rs}`, `ClipForge-Server` hygiene | Finalized `ClipForge-Server` commit (`b4d32e0`), ran read-only `s3.emkacz.dev` `storage check`, and updated `docs/` |
| `server-init` | Server Extraction & API | `ClipForge-Server/crates/clipforge-api` | Initialized standalone `ClipForge-Server` workspace + OpenAPI 3.1.0 `/api/v1` DTOs (`d33e4ab`) |
| `srv-db-auth` | Server DB & Auth | `ClipForge-Server/{migrations,crates/clipforge-{db,auth}}` | SQLite schema/repos, Argon2id, SHA-256 personal API tokens, quota & cross-user isolation |
| `srv-storage` | Server Storage | `ClipForge-Server/crates/clipforge-storage` | `LocalStorage` + RustFS `S3Storage` (SigV4, Range, multipart) + `storage check` diagnostics |
| `srv-web` | Server Web Player | `ClipForge-Server/web/index.html` | Dark-first `/c/{share_id}` HTML5 `<video>` player with HTTP Range seeking |
| `srv-http` | Server HTTP & Packaging | `ClipForge-Server/{crates/clipforge-server,Dockerfile,compose.yaml,docs}` | Axum `/api/v1` router, CLI, `Dockerfile`/`compose.yaml`/CI/docs (`a77bc5c`, `b4d32e0`) |
| `desktop-cloud` | Desktop `/api/v1` & Secrets | `ClipForge/{crates/cloud-client,apps/desktop}` | Upgraded `cloud-client` & `views/uploads.rs` to `/api/v1`, `secret-tool` + `0600` `credentials.json` (`786d61a`) |
| `qa` | QA & Security | Read-only verification | Verified `ClipForge` (102+3 ignored), `ClipForge-Server` (22 tests), live E2E on port `44627`, `--snap` (`#150`) |

## Remaining Limitations & Follow-Ups

1. **Interactive Windowed & Live S3 Verification:** Live voice `Test Microphone` + `Play Sample`, KDE `GlobalShortcuts` portal approval + unfocused `F9`/`F10`/`F11`, and authenticated `clipforge-server storage check --write-probe` against `https://s3.emkacz.dev` remain to be run manually.
2. **Sample origin migration, CI & Publishing:** `apps/desktop` can switch from `"sample"` tag to `ClipOrigin::Sample`; `ClipForge` CI needs `libxkbcommon-dev` + `libasound2-dev`; neither repo has been pushed. See `docs/NEXT_TASKS.md`.
