# ClipForge Next Tasks

Ordered tasks remaining after verified commits `786d61a` (`ClipForge`) and `b4d32e0` (`ClipForge-Server`):

1. **Manual Interactive Windowed Auditioning & Wayland Hotkey Verification:**
   - Run `cargo run -p clipforge-desktop` interactively on KDE Wayland and verify:
     - `Test Microphone` with live voice (`GoodSignal` classification) + audible `Play Sample` (`cpal`), and `Restart Replay Buffer` banner after toggling audio sources/tracks while active.
     - Approve the KDE `GlobalShortcuts` portal dialog and test `F9` (Save Replay), `F10` (Toggle Replay Buffer), and `F11` (Start/Stop Recording) while ClipForge is **unfocused**.
     - Live multi-frame embedded playback (`Auto` mixed multi-track audio), pointer-drag timeline/trim/volume UX, and `Shared & Cloud` live upload/share flow against `ClipForge-Server`.
2. **RustFS / S3 Authenticated Storage Verification (`ClipForge-Server`):**
   - Configure `S3_ACCESS_KEY_ID` and `S3_SECRET_ACCESS_KEY` in the target server environment and run `clipforge-server storage check --write-probe` against `https://s3.emkacz.dev` (`S3_BUCKET=clipforge`) to verify bucket permissions and roundtrip object write/read/delete.
3. **Migrate `apps/desktop` Sample Tracking to `ClipOrigin::Sample`:**
   - Switch `apps/desktop` (`state.rs`, `worker.rs`) from the `"sample"` tag convention to `crates/library`'s v2 `ClipOrigin::Sample` / `list_clips_filtered` APIs.
4. **Packaging, CI & Repository Publishing (Milestone 6):**
   - Validate `ClipForge/packaging/arch/PKGBUILD` (`makepkg`), `ClipForge-Server/Dockerfile` + `compose.yaml`, and `.github/workflows/ci.yml` (ensure `libxkbcommon-dev` and `libasound2-dev` are installed on Ubuntu CI runners for `ClipForge`).
   - Push `ClipForge` and `ClipForge-Server` `main` branches to their remotes when ready.
