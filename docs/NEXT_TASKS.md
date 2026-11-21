# ClipForge Next Tasks

Ordered tasks remaining after `ClipForge` (`1afbd80`) and `ClipForge-Server` (`8b6326d`):

1. **RustFS Production Storage Cutover (Separate Approval Required):**
   - Create dedicated `clipforge` bucket and scoped service account credentials on RustFS (`111 rustfs` / `https://s3.emkacz.dev`).
   - Run authenticated `clipforge-server storage check --write-probe` from LXC `112` and switch LXC `112` `.env` from `local` to `s3` (`S3_ENDPOINT=https://s3.emkacz.dev`, `S3_BUCKET=clipforge`).
2. **Cloudflare & Public Route Cutover (`clips.emkacz.dev`, Separate Approval Required):**
   - Configure Cloudflare Tunnel (`102 cf-tunnel`) / Nginx Proxy Manager (`108`) ingress for `clips.emkacz.dev -> http://192.168.1.112:3000` (with Zero Trust / upload timeout & chunking verification) and update `CLIPFORGE_PUBLIC_BASE_URL=https://clips.emkacz.dev`.
   - Disable open registration (`CLIPFORGE_REGISTRATION_ENABLED=false`) after creating production user/admin accounts.
3. **Manual Interactive Windowed Auditioning on CachyOS Wayland:**
   - Run `cargo run -p clipforge-desktop` interactively on KDE Wayland:
     - `Test Microphone` with live voice (`GoodSignal`) + audible `Play Sample` (`cpal`).
     - Approve KDE `GlobalShortcuts` portal dialog and test unfocused `F9`/`F10`/`F11`.
     - Live multi-frame embedded playback (`Auto` mixed multi-track audio), pointer-drag timeline/trim/volume UX, and live desktop-to-LXC-112 upload/share flow (`secret-tool` token storage).
4. **Repository Hygiene & Publishing:**
   - Decide whether to commit or ignore untracked design artifacts in `ClipForge` (`design/`, `docs/design.md`, `docs/ui-implementation-plan.md`, `docs/ui-screen-specs.md`).
   - Migrate `apps/desktop` sample tracking from `"sample"` tag to `ClipOrigin::Sample`.
   - Validate `ClipForge/packaging/arch/PKGBUILD` (`makepkg`), update `.github/workflows/ci.yml` (`libxkbcommon-dev`, `libasound2-dev`), and push both `ClipForge` and `ClipForge-Server` `main` branches when ready.
