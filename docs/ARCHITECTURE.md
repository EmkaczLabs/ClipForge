# ClipForge Architecture

## Overview

ClipForge is organized as a Cargo workspace (`resolver = "2"`, Rust `edition = "2024"`) separating pure domain contracts from platform-specific backends, media tooling, persistence, and UI/server applications.

```text
+---------------------------------------------------------------+
|                         apps/desktop                          |
|                    (Rune GPU-rendered UI)                     |
+-------+---------------+---------------+---------------+-------+
        |               |               |               |
        v               v               v               v
+--------------+ +-------------+ +-------------+ +--------------+
|crates/capture| |crates/media | |crates/      | |crates/       |
|(GSR backend) | |(ffmpeg CLI) | |library      | |cloud-client  |
+------+-------+ +------+------+ |(SQLite)     | |(reqwest)     |
       |                |        +------+------+ +------+-------+
       |                |               |               |
       +----------------+-------+-------+---------------+
                                |
                                v
                        +---------------+
                        | crates/common |
                        | (models,      |
                        |  traits, err) |
                        +---------------+
                                ^
                                |
                        +-------+-------+
                        |  apps/server  |
                        |  (Axum API)   |
                        +---------------+
```

## Core Contracts (`crates/common`)

`crates/common` (`clipforge-common`) defines the shared types, error enum, tracing initializer, and async trait boundaries:

- **Error & Logging**:
  - `ClipForgeError` (`Io`, `Serialization`, `NotFound`, `InvalidInput`, `CaptureUnavailable`, `CaptureFailed`, `MediaFailed`, `Database`, `Cloud`, `Other`) and `Result<T>` in `src/error.rs`.
  - `init_tracing(default_level: &str)` in `src/logging.rs`.
- **Domain Models** (`src/models.rs`):
  - `ClipId`, `ClipMetadata`, `GameInfo`, `AudioTrackInfo`, `CloudShareInfo`
  - `CaptureConfig`, `CaptureTarget`, `VideoCodec`, `AudioCodec`, `ContainerFormat`, `QualityPreset`, `EncoderMode`
  - `CaptureStatus`, `CaptureEvent`
  - `MediaProbeResult`, `TrimRequest`, `ClipFilter`, `AppConfig`
- **Async Trait Boundaries** (`src/traits.rs`):
  - `CaptureBackend`: `is_available`, `start_replay_buffer`, `save_replay`, `start_recording`, `stop`, `status`, `subscribe` (`broadcast::Receiver<CaptureEvent>`)
  - `MediaProcessor`: `probe`, `generate_thumbnail`, `trim`
  - `ClipStore`: `upsert_clip`, `get_clip`, `list_clips`, `delete_clip`, `add_tag`, `remove_tag`
  - `CloudClient`: `health_check`, `upload_clip`, `get_share`, `delete_share`

## Crate Responsibilities

- **`crates/capture` (`clipforge-capture`)**: Implements `CaptureBackend` by spawning and supervising `gpu-screen-recorder`. Handles replay-save signals (`SIGUSR1`), clean shutdown (`SIGINT`), and stdout/hook event parsing.
- **`crates/media` (`clipforge-media`)**: Implements `MediaProcessor` via `ffprobe` (JSON metadata) and `ffmpeg` (JPEG/PNG thumbnails, fast stream-copy or re-encoded trimming).
- **`crates/library` (`clipforge-library`)**: Implements `ClipStore` using SQLite (`rusqlite` with `bundled` feature) and manages clip/tag queries.
- **`crates/cloud-client` (`clipforge-cloud-client`)**: Implements `CloudClient` using `reqwest` (`rustls-tls`) to upload clips and manage share links against `apps/server`.
- **`apps/desktop` (`clipforge-desktop`)**: Native desktop UI built with `anomalyco/rune`, driving the crates asynchronously without blocking the UI render loop.
- **`apps/server` (`clipforge-server`)**: Axum HTTP server providing clip upload, metadata, and share-link endpoints.
