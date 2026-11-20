# Development Guide

## Environment Setup

1. Install Rust (Edition 2024 supported, `rustc 1.99+`):
   ```bash
   rustup update
   ```
2. Install runtime tools (for capture & media features):
   - `ffmpeg` and `ffprobe` (e.g. `sudo apt install ffmpeg`)
   - `gpu-screen-recorder` (via distro package or Flatpak `com.dec05eba.gpu_screen_recorder`)
3. Node.js 22+ & `pnpm` (for `apps/web` when added).

## Workspace Build Notes

- **Rune Git Dependency & `[patch.crates-io]`**: `apps/desktop` depends on `anomalyco/rune` pinned to commit `dc8f7f7213cbe43af45212de6998d530d0945187`. The root `Cargo.toml` includes the required `[patch.crates-io]` overrides (`wgpu-hal`, `winit`, `sctk-adwaita`, `parley_engine`, `accesskit_windows`, `accesskit_macos`).
- **Debug Profile**: Root `Cargo.toml` sets `[profile.dev] debug = "line-tables-only"` and `[profile.dev.package."*"] debug = false` to keep debug build memory usage low.
- **Bundled SQLite**: `rusqlite` is configured with `features = ["bundled"]`, so `libsqlite3-dev` is not needed on the host.

## Common Commands

```bash
# Format check
cargo fmt --all -- --check

# Type-check all workspace crates
cargo check --workspace

# Run tests for a specific crate
cargo test -p clipforge-common

# Run headless UI snapshot (Rune tooling feature in debug builds)
cargo run -p clipforge-desktop -- --snap /tmp/clipforge.png
```
