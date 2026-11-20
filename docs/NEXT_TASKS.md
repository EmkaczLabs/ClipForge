# ClipForge Next Tasks

Ordered tasks remaining after verified commit `a30157e` on CachyOS (KDE Plasma / Wayland):

1. **Migrate `apps/desktop` Sample Tracking to `ClipOrigin::Sample`:**
   - Switch `apps/desktop` (`state.rs`, `worker.rs`) from the `"sample"` tag convention to `crates/library`'s v2 `ClipOrigin::Sample` / `list_clips_filtered` APIs.
2. **Wayland `FocusedWindow` Guard & UX:**
   - Detect Wayland sessions (`XDG_SESSION_TYPE=wayland`) in `apps/desktop` and disable or warn on `CaptureTarget::FocusedWindow` (`-w focused` is X11-only in GSR 6.1.7), steering users to `PrimaryScreen`, `Monitor`, `Region`, or `Portal`.
   - Surface codec fallback warnings when `VideoCodec::Av1` (unsupported NVENC path on RTX 3070) or `AudioCodec::Flac` is selected.
3. **Wayland Global Hotkeys:**
   - Implement XDG Desktop Portal `GlobalShortcuts` support (alongside `HotkeyManager::start_x11`) or compositor shortcut integration for Wayland replay-save hotkeys.
4. **End-to-End Self-Hosted Sharing Verification (Milestone 5):**
   - Start `cargo run -p clipforge-server` (or `packaging/docker/docker-compose.yml`), upload a real GSR-recorded clip from `apps/desktop`, and verify playback/seeking in `apps/web/index.html`.
5. **Packaging & CI Validation (Milestone 6):**
   - Validate `packaging/arch/PKGBUILD` (`makepkg`), `packaging/docker/docker-compose.yml`, and `.github/workflows/ci.yml` (ensure `libxkbcommon-dev` is installed on Ubuntu CI runners for `clipforge-desktop`).
