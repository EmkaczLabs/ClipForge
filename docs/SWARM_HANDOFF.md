# ClipForge Swarm Handoff

## 1. Environment & Repository State

- **Host:** CachyOS (KDE Plasma / Wayland), NVIDIA RTX 3070, `gpu-screen-recorder 6.1.7`, `rustc 1.100.0`, `cargo 1.100.0`, `ffmpeg n9.0.2`, `secret-tool` (`org.freedesktop.secrets`).
- **Git State (`ClipForge` on `main`, NOT Pushed):**
  - `HEAD = aae63b7` (`a1ec416` `windows 0.62` deps -> `4f01cc3` `windows_wgc.rs` & `windows_audio.rs` -> `88d7631` `windows_encoder.rs`, `windows_replay.rs`, `windows_backend.rs`, `discovery.rs`, `hotkey.rs` -> `aae63b7` `CREATE_NO_WINDOW`, `windows_subsystem = "windows"`, file logging, desktop Windows backend wiring).
  - Modified/untracked on disk: `Cargo.lock` (from `windows 0.62` resolution) and preserved design artifacts (`design/`, `docs/design.md`, `docs/ui-implementation-plan.md`, `docs/ui-screen-specs.md`).
- **Git State (`ClipForge-Server` on `main`, NOT Pushed):** `HEAD = 9216eb2` (includes `7ef9197` OG/Twitter/oEmbed link previews + `9216eb2` storage-first `DELETE` & visibility sync).

## 2. Swarm Workers & Latest Verification

- **Verified on Linux (`aae63b7`):** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `XDG_CONFIG_HOME=/tmp/clipforge-test-cfg cargo test --workspace` (**138 passed, 0 failed, 3 ignored**), and `--scene recording --snap /tmp/clipforge-recording-wgc.png` (**390,547 B**, zero Rune warnings).
- **Recent Workers:**
  - `win-crash` (`aa6825c`): Isolated `cpal` audio streams onto persistent `"clipforge-cpal"` OS thread to fix Windows Server 2025 `0xC0000005` WASAPI COM/thread-exit crash.
  - `win-wgc-audio` (`4f01cc3`): Implemented `windows_wgc.rs` (D3D11/DXGI/WGC) and `windows_audio.rs` (WASAPI loopback + mic on `COINIT_MULTITHREADED` threads).
  - `win-capture` (`88d7631`): Implemented `windows_encoder.rs`, `windows_replay.rs`, `windows_backend.rs`, `discovery.rs`, and `RegisterHotKey`.
  - `win-desktop` (`aae63b7`): Added `windows_subsystem = "windows"`, file logging, `CREATE_NO_WINDOW` across all subprocesses, and Windows UI wiring.
  - `qa` (`#260`): Verified 138+3 ignored tests, `--snap`, and `CREATE_NO_WINDOW` coverage.

## 3. Remaining Validation & Cutovers

1. **`windows-latest` CI & Physical Windows 10/11 Hardware:** Live Windows compilation of WGC/WASAPI/`RegisterHotKey`, `cpal` `0xC0000005` verification, WGC monitor/window capture, WASAPI loopback+mic A/V sync, replay buffer save, `RegisterHotKey`, and portable ZIP execution await push to GitHub Actions / Windows hardware.
2. **LXC 112 / RustFS / Cloudflare & Wayland Auditioning:** See `docs/NEXT_TASKS.md`.
