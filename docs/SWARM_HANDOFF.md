# ClipForge Swarm Handoff

## 1. Environment & Repository State

- **Host:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`, `secret-tool` (`org.freedesktop.secrets`).
- **Proxmox Server Host (`pve` / `192.168.1.57`):** LXC `112` (`192.168.1.112:3000`) running `clipforge-server` (`docker compose`, `healthy`, `/ready` `200 OK`) in local-storage test mode, included in `/etc/pve/jobs.cfg`. Unrelated CTs (`102`, `108`, `111`, `116`) untouched and running.
- **Git State (Both Repos on `main`, NOT Pushed):**
  - **`/home/emkacz/Projects/ClipForge`**: `HEAD = 1afbd80` (`main`), single worktree. Untracked design artifacts preserved on disk (not committed): `design/`, `docs/design.md`, `docs/ui-implementation-plan.md`, `docs/ui-screen-specs.md`.
  - **`/home/emkacz/Projects/ClipForge-Server`**: `HEAD = 8b6326d` (`main`), single worktree, working tree completely clean.

### Recent Git History
- `ClipForge`: `1afbd80` (token hardening) <- `3413490` (docs) <- `786d61a` (`/api/v1` desktop cloud) <- `6ed8d98` (docs) <- `1592c5a` (studio UI) <- `636fce3` (routing/mic state) <- `3674bce` (`AutoMix` + `MicTestHandle`) <- `8ff12e8` (`Auto`/`T1`–`T4` GSR routing).
- `ClipForge-Server`: `8b6326d` (healthcheck `/ready` & Compose log rotation) <- `b4d32e0` (db/auth/storage/web/docs/CI) <- `a77bc5c` (HTTP `/api/v1` + CLI) <- `bba2acb` (`argon2`) <- `d33e4ab` (workspace + `clipforge-api`).

## 2. Swarm Workers & Status (`ClipForge — LXC 112 Deployment & Hardening`)

- **Board / Coordinator Session:** `857becd0-915f-4dc1-b49e-eb2c59b476cf` (title: `ClipForge — LXC 112 Deployment & Hardening`, head `#173`).
- **Active (`waiting` / `working`):**
  - `coordinator` (`waiting`, last post `#173`) — Swarm orchestration
  - `arch` (`working`, owns `Cargo.toml`, `crates/common/`, `docs/`, `AGENTS.md`) — Architecture, integration & handoff docs
  - `infra` (`waiting`, last post `#166`) — Proxmox LXC `112` provisioning & backup job
  - `net` (`waiting`, last post `#165`) — Cloudflare Tunnel (`102`), NPM (`108`), RustFS (`111`) read-only audit
  - `deploy` (`waiting`, last post `#169`) — Docker/Compose deployment on LXC `112` & token hardening (`1afbd80`, `8b6326d`)
  - `qa` (`waiting`, last post `#171`) — Read-only verification across `ClipForge`, `ClipForge-Server`, and LXC `112`
  - `monitor` (`waiting`, last post `#167`) — Stall monitor
- **Completed & Killed (`killed: true`, all work landed on `main`):**
  - Desktop/Media/Capture: `gsr`, `media`, `ui`, `media-dev`, `ui-dev`, `editor-dev`, `decoder`, `renderer`, `timeline`, `lib-store`, `lib-ui`, `design`, `shell`, `library-ui`, `editor-ui`, `recording-ui`, `sharing-ui`, `settings-ui`, `audio-ui`, `hotkeys`, `audio-capture`, `mic-backend`, `studio-ui`.
  - Server Extraction: `server-init`, `srv-db-auth`, `srv-storage`, `srv-web`, `srv-devops`, `desktop-cloud`.

## 3. Blockers & Cross-Machine Migration Notes

- **Antigravity Conversation Migration:** Conversation IDs (`857becd0-915f-4dc1-b49e-eb2c59b476cf`, etc.) and live worker sessions **cannot** be migrated between machines. When resuming on another machine, start a fresh Antigravity session/swarm against the committed `main` branches (`1afbd80` and `8b6326d`).
- **Pending Production Cutovers:** RustFS (`111`) bucket/credentials and Cloudflare (`102`) `clips.emkacz.dev` routing were intentionally left untouched pending user approval.
- **Unverified Interactive Desktop Flows:** Live voice `Test Microphone` + `Play Sample`, KDE `GlobalShortcuts` approval + unfocused `F9`/`F10`/`F11`, and interactive Wayland pointer-drag UX remain to be tested in a desktop window.

## 4. Resuming & Recommended Next Swarm Assignments

1. **Pull/sync** both `/home/emkacz/Projects/ClipForge` (`main`) and `/home/emkacz/Projects/ClipForge-Server` (`main`).
2. **Verify baseline:** Run `cargo test --workspace` in both repos.
3. **Recommended Next Swarm:**
   - `@net` / `@deploy`: Provision RustFS `clipforge` bucket + scoped key, run `storage check --write-probe`, switch LXC `112` to `s3` backend, and configure `clips.emkacz.dev` via Cloudflare Tunnel (`102`) / NPM (`108`).
   - `@qa`: Verify end-to-end desktop (`secret-tool` + `https://clips.emkacz.dev`) upload, share playback, and `Range` streaming.
   - `@arch`: Commit or `.gitignore` `design/` + `docs/{design,ui-*}.md`, migrate `ClipOrigin::Sample`, and validate `PKGBUILD` / CI.
