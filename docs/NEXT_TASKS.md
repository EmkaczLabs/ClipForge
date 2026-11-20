# ClipForge Next Tasks

Ordered tasks to execute upon resuming (e.g., on CachyOS):

1. **Install Desktop Link & Runtime Dependencies (CachyOS / Arch):**
   - Ensure `libxkbcommon`, `vulkan-icd-loader`, `ffmpeg`, `dbus`, and `gpu-screen-recorder` (AUR / Flatpak / repo) are installed.
   - Verify `pkg-config` / linker finds `-lxkbcommon` (`/usr/lib/libxkbcommon.so`).
2. **Unblock & Verify Milestone 1 (`apps/desktop`):**
   - Run `cargo test -p clipforge-desktop` (verifies `rune::Runtime` UI tests + config tests).
   - Run `cargo run -p clipforge-desktop -- --snap /tmp/clipforge-m1.png` and inspect the rendered screenshot.
   - Launch `cargo run -p clipforge-desktop` interactively on the desktop session.
3. **End-to-End Recording Verification (Milestone 2):**
   - With `gpu-screen-recorder` installed on CachyOS, test `detect_gsr()`, monitor/audio discovery, starting replay buffer, saving a replay clip via UI and global hotkey, and stopping cleanly.
4. **End-to-End Library & Editor Verification (Milestones 3 & 4):**
   - Verify newly saved GSR clips are auto-scanned into SQLite, thumbnails generate without blocking UI, and trimming exports a valid playable clip with audio preserved.
5. **End-to-End Self-Hosted Sharing Verification (Milestone 5):**
   - Start `cargo run -p clipforge-server` (or `packaging/docker/docker-compose.yml`), upload a clip from `apps/desktop`, and play the share link in a browser (`apps/web/index.html`).
6. **Packaging & CI Validation (Milestone 6):**
   - Validate `packaging/arch/PKGBUILD` (`makepkg`), `packaging/docker/docker-compose.yml`, and GitHub Actions workflow `.github/workflows/ci.yml` (add `libxkbcommon-dev` to CI `apt-get install`).
