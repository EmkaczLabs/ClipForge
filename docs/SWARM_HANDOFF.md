# ClipForge Swarm Handoff

## 1. Environment & Repository State

- **Host:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`, `secret-tool` (`org.freedesktop.secrets`).
- **Proxmox Server Host (`pve` / `192.168.1.57`):** LXC `112` (`192.168.1.112:3000`) running `clipforge-server` (`8b6326d`, `healthy`, `/ready` `200 OK`) in local-storage test mode, included in `/etc/pve/jobs.cfg`. **Needs rebuild/redeploy from `ClipForge-Server` `9216eb2` (once approved)** to pick up storage-before-DB `DELETE` and `shares.visibility` sync. Unrelated CTs (`102`, `108`, `111`, `116`) untouched.
- **Git State (Both Repos on `main`, NOT Pushed):**
  - **`/home/emkacz/Projects/ClipForge`**: `7ef7763` + `apps/server` `PATCH /api/v1/clips/{id}` fix + docs commit on `main`, single worktree. Untracked design artifacts preserved on disk: `design/`, `docs/design.md`, `docs/ui-implementation-plan.md`, `docs/ui-screen-specs.md`.
  - **`/home/emkacz/Projects/ClipForge-Server`**: `HEAD = 9216eb2` (`main`), single worktree, working tree clean.

## 2. Swarm Workers & Status

- **Board / Coordinator Session:** `857becd0-915f-4dc1-b49e-eb2c59b476cf`.
- **Recent Fixes Landed:**
  - `text-input` (`a980e92`): Fixed `rune::text_input` `.on_edit` focus, `.single_line().placeholder`, `.focus_within_ring`, and `.secure()` masking.
  - `srv-fix` (`9216eb2` in `ClipForge-Server`): Fixed `PATCH /api/v1/clips/{id}` `shares.visibility` sync, added `PATCH /api/v1/shares/{token}/visibility`, and made `DELETE /api/v1/clips/{id}` delete storage objects before SQLite rows.
  - `client-fix` (`7ef7763` in `ClipForge`): Keyed visibility & remote delete on `clip_id` (`PATCH/DELETE /api/v1/clips/{clip_id}`), removed `delete_share` fallback, gated UI mutations on server `Ok`, and kept Delete/Visibility/Create Share on refreshed unshared clips.
  - `arch`: Added `PATCH /api/v1/clips/{id}` to legacy `apps/server/src/routes.rs` + updated `apps/server/tests/integration_tests.rs` so `cargo test --workspace` passes 100% (`108 passed, 0 failed, 3 ignored` with `XDG_CONFIG_HOME=/tmp/clipforge-test-cfg`).

## 3. Blockers & Cross-Machine Migration Notes

- **Antigravity Conversation Migration:** Conversation IDs (`857becd0-915f-4dc1-b49e-eb2c59b476cf`) and live worker sessions **cannot** be migrated between machines. Start a fresh Antigravity session against `main` when resuming on another machine.
- **Pending Production Cutovers (Separate Approval Required):**
  1. Rebuild/redeploy LXC `112` from `ClipForge-Server` `9216eb2`.
  2. Create `clipforge-svc` + `clipforge-rw` policy in RustFS Console (`https://s3-web.emkacz.dev/rustfs/console/index.html`) and switch LXC `112` to `s3` backend (`storage check --write-probe`).
  3. Route `https://clips.emkacz.dev` via Cloudflare Tunnel (`102`) / NPM (`108`) and disable open registration.
- **Unverified Interactive Desktop Flows:** Live voice `Test Microphone` + `Play Sample`, KDE `GlobalShortcuts` approval + unfocused `F9`/`F10`/`F11`, and interactive Wayland pointer-drag UX remain to be tested in a desktop window.
