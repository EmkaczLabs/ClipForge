# ClipForge

**ClipForge** is an open-source Linux game clipping, replay-buffer, editing, and sharing application written in Rust.

## Features

- **Low-overhead Replay Buffer & Recording**: Powered by [GPU Screen Recorder](https://git.dec05eba.com/gpu-screen-recorder/) (`gpu-screen-recorder`) with hardware encoding (NVENC, VAAPI, Vulkan), X11 and Wayland (`xdg-desktop-portal` + PipeWire) support, and multi-track audio capture.
- **Native GPU-Rendered Desktop UI**: Built with [Rune](https://github.com/anomalyco/rune) (`anomalyco/rune`), a native GPU-rendered Rust UI framework.
- **Local Clip Library**: SQLite-backed clip index (`rusqlite` bundled), game/tag filtering, search, and automatic thumbnailing.
- **Fast Media Processing**: `ffmpeg` / `ffprobe` integration for clip probing, thumbnail generation, and lossless or re-encoded trimming.
- **Self-Hostable Cloud Sharing**: Optional `clipforge-server` (Axum) and web share viewer for uploading and sharing clips via links.

## Workspace Structure

```text
ClipForge/
├── apps/
│   ├── desktop/          # clipforge-desktop — Rune GPU-rendered desktop UI
│   └── server/           # clipforge-server — Axum cloud sharing API
└── crates/
    ├── common/           # clipforge-common — shared models, errors, tracing, async traits
    ├── capture/          # clipforge-capture — gpu-screen-recorder CaptureBackend
    ├── media/            # clipforge-media — ffmpeg/ffprobe MediaProcessor
    ├── library/          # clipforge-library — SQLite ClipStore
    └── cloud-client/     # clipforge-cloud-client — reqwest CloudClient
```

## Prerequisites

- **Rust**: Edition 2024 toolchain (`rustc 1.99+`, `cargo`)
- **Runtime tools**:
  - `ffmpeg` and `ffprobe` (for media probing, thumbnails, and trimming)
  - `gpu-screen-recorder` (native package or Flatpak `com.dec05eba.gpu_screen_recorder`, required at runtime for screen/game capture)
  - Vulkan ICD + X11 or Wayland + D-Bus (for the Rune desktop UI)
- **Linux compile-time note**: Rune uses `dlopen` for X11/Wayland/Vulkan and `rusqlite` uses the `bundled` feature, so no extra `-dev` C headers are required to build the Rust workspace.

## Build & Run

```bash
# Check the entire workspace
cargo check --workspace

# Run unit tests
cargo test --workspace

# Run the desktop application
cargo run -p clipforge-desktop

# Run the cloud sharing server
cargo run -p clipforge-server
```

## License

ClipForge is licensed under the [MIT License](LICENSE).
