# ClipForge Next Tasks

Ordered tasks remaining after `ClipForge` (`aae63b7`) and `ClipForge-Server` (`9216eb2`):

1. **Windows CI & Physical Windows 10/11 Validation (`windows-latest` + Hardware):**
   - Push `ClipForge` `main` (when ready) to trigger `.github/workflows/ci.yml` (`windows-test` validating `#[cfg(target_os = "windows")]` WGC/WASAPI/`RegisterHotKey` compilation and the `aa6825c` `cpal` COM fix on Windows Server 2025) and `.github/workflows/windows-portable.yml`.
   - Test the portable ZIP (`ClipForge-Windows-x64-portable.zip`) on physical Windows 10/11 hardware:
     - Verify zero console window flash (`windows_subsystem = "windows"` + `CREATE_NO_WINDOW`).
     - Verify WGC monitor/focused-window capture, WASAPI loopback + mic A/V sync, rolling replay buffer save, `RegisterHotKey` (`F9`/`F10`/`F11`), and FFmpeg HW/SW encoding.
2. **RustFS & LXC 112 Production Cutover (Separate Approval Required):**
   - Rebuild/redeploy LXC `112` from `ClipForge-Server` `9216eb2` (storage-before-DB `DELETE` + `shares.visibility` sync) + `7ef9197` (OG/Twitter/oEmbed link previews).
   - Create `clipforge-svc` + `clipforge-rw` policy in RustFS Console (`https://s3-web.emkacz.dev/rustfs/console/index.html`), run `clipforge-server storage check --write-probe`, and switch LXC `112` to `s3` backend.
   - Route `https://clips.emkacz.dev` via Cloudflare Tunnel (`102`) / NPM (`108`) and disable open registration.
3. **Manual Interactive Windowed Auditioning on CachyOS Wayland:**
   - Test `Test Microphone` with live voice + `Play Sample`, KDE `GlobalShortcuts` portal approval + unfocused `F9`/`F10`/`F11`, and pointer-drag editor UX.
4. **Repository Hygiene:**
   - Commit `Cargo.lock` update for `windows 0.62` when appropriate, decide whether to commit or `.gitignore` `design/` + `docs/{design,ui-*}.md`, and migrate `apps/desktop` sample tracking from `"sample"` tag to `ClipOrigin::Sample`.
