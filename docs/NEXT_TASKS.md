# ClipForge Next Tasks

Ordered tasks remaining after verified commit `56099f1` on CachyOS (KDE Plasma / Wayland):

1. **Manual Interactive Windowed & Unfocused Wayland Hotkey Verification:**
   - Run `cargo run -p clipforge-desktop` interactively on KDE Wayland and verify:
     - Approve the KDE `GlobalShortcuts` portal permission dialog (confirm `Global Hotkeys` card transitions to `Portal Active`) and test `F9` (Save Replay), `F10` (Toggle Replay Buffer), and `F11` (Start/Stop Recording) while ClipForge is **unfocused**.
     - PipeWire/ALSA audio auditioning, mute toggle, volume slider drag, and multi-track selector.
     - Live multi-frame embedded playback, pointer-drag timeline scrubbing, trim-in/trim-out handles, `1x..8x` zoom scroll, and keyboard shortcuts (`Space`, `Left`/`Right`).
     - Recording Studio `Refresh Sources` (`DiscoverCaptureSources`) with long PipeWire device names, Wayland Portal / Primary Screen replay save, and Settings persistence across restarts.
2. **End-to-End Self-Hosted Sharing Verification (Milestone 5):**
   - Start `cargo run -p clipforge-server` (or `packaging/docker/docker-compose.yml`), connect from `Shared & Cloud` (`views/uploads.rs`), upload a real GSR-recorded clip, verify `Copy Link` / `Open` / visibility toggle / remote delete confirmation, and test playback/seeking in `apps/web/index.html`.
3. **Migrate `apps/desktop` Sample Tracking to `ClipOrigin::Sample`:**
   - Switch `apps/desktop` (`state.rs`, `worker.rs`) from the `"sample"` tag convention to `crates/library`'s v2 `ClipOrigin::Sample` / `list_clips_filtered` APIs.
4. **Packaging & CI Validation (Milestone 6):**
   - Validate `packaging/arch/PKGBUILD` (`makepkg`), `packaging/docker/docker-compose.yml`, and `.github/workflows/ci.yml` (ensure `libxkbcommon-dev` and `libasound2-dev` are installed on Ubuntu CI runners for `clipforge-desktop` and `cpal`).
