# ClipForge Next Tasks

Ordered tasks remaining after verified commit `40311c8` on CachyOS (KDE Plasma / Wayland):

1. **Manual Interactive Windowed Verification (Wayland):**
   - Run `cargo run -p clipforge-desktop` interactively on KDE Wayland and verify PipeWire/ALSA audio auditioning, mute/volume slider drag, live multi-frame playback + A/V sync, pointer-drag timeline/trim handles, `1x..8x` zoom scroll, keyboard shortcuts (`Space`, `Left`/`Right`), export progress/cancel, and Library management UX.
2. **Migrate `apps/desktop` Sample Tracking to `ClipOrigin::Sample`:**
   - Switch `apps/desktop` (`state.rs`, `worker.rs`) from the `"sample"` tag convention to `crates/library`'s v2 `ClipOrigin::Sample` / `list_clips_filtered` APIs.
3. **Wayland `FocusedWindow` Guard & Codec UX:**
   - Detect Wayland sessions (`XDG_SESSION_TYPE=wayland`) in `apps/desktop` and disable or warn on `CaptureTarget::FocusedWindow` (`-w focused` is X11-only in GSR 6.1.7), steering users to `PrimaryScreen`, `Monitor`, `Region`, or `Portal`.
   - Surface codec fallback warnings when `VideoCodec::Av1` (unsupported NVENC path on RTX 3070) or `AudioCodec::Flac` is selected.
4. **Wayland Global Hotkeys:**
   - Implement XDG Desktop Portal `GlobalShortcuts` support (alongside `HotkeyManager::start_x11`) or compositor shortcut integration for Wayland replay-save hotkeys.
5. **End-to-End Self-Hosted Sharing Verification (Milestone 5):**
   - Start `cargo run -p clipforge-server` (or `packaging/docker/docker-compose.yml`), upload a real GSR-recorded clip from `apps/desktop`, and verify playback/seeking in `apps/web/index.html`.
6. **Packaging & CI Validation (Milestone 6):**
   - Validate `packaging/arch/PKGBUILD` (`makepkg`), `packaging/docker/docker-compose.yml`, and `.github/workflows/ci.yml` (ensure `libxkbcommon-dev` and `libasound2-dev` are installed on Ubuntu CI runners for `clipforge-desktop` and `cpal`).
