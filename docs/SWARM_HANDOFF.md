# ClipForge Swarm Handoff

## Swarm State at Checkpoint

All implementation and QA workers reached a clean `waiting` state prior to this checkpoint commit:

| Member | Role | State at Checkpoint | Last Completed Work |
|---|---|---|---|
| `arch` | Architecture & Integration | `waiting` | Workspace skeleton, `crates/common`, docs suite, `packaging/` templates (#11, #18, #29) |
| `ui` | Rune Desktop UI | `waiting` | `apps/desktop` 6-section Rune UI + backend `WorkerHandle` wiring (#24, #30) |
| `gsr` | GSR Integration | `waiting` | `crates/capture` GSR discovery, arg builder, `SIGUSR1`/`SIGINT` supervision, X11 hotkey (#19, #22) |
| `media` | Media & Library | `waiting` | `crates/media` (`ffmpeg`/`ffprobe` trim/thumb) & `crates/library` (`rusqlite` bundled + scanner) (#21) |
| `cloud` | Cloud & Backend | `waiting` | `apps/server` (Axum + SQLite + local/S3 + Range), `crates/cloud-client`, `apps/web` (#23) |
| `qa` | QA & Security | `waiting` | Verified `fmt`, `clippy -D warnings`, 39 non-desktop tests, and security review (#28, #31) |
| `monitor` | Stall Monitor | `waiting` / timer | Active stall monitor |

## Known Blockers

1. **Missing `libxkbcommon-dev` on Linux Mint host:** `rune` depends on `xkbcommon v0.9.0`, which links `-lxkbcommon`. On CachyOS, `libxkbcommon` includes headers/`.so` symlink by default.
2. **`gpu-screen-recorder` not installed on Linux Mint host:** `crates/capture` passes unit/mock-process tests; real GSR capture requires `gpu-screen-recorder` installed on the target machine.
3. **CI workflow dependency:** `.github/workflows/ci.yml` will also need `libxkbcommon-dev` installed on Ubuntu runners to link `clipforge-desktop`.

## Resuming on Another Machine (CachyOS)

1. Pull/sync the local `main` branch commit from this machine (or copy the git repo).
2. Install Rust toolchain (`rustup` / `rustc >= 1.99` for `edition = "2024"`), `ffmpeg`, `libxkbcommon`, `vulkan-icd-loader`, and `gpu-screen-recorder`.
3. Run:
   ```bash
   cargo test --workspace
   cargo run -p clipforge-desktop -- --snap /tmp/clipforge-m1.png
   ```
4. Continue with `docs/NEXT_TASKS.md`.
