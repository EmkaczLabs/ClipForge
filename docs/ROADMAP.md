# ClipForge Roadmap

## Milestone 1 — Workspace & UI Shell
- [x] Cargo workspace skeleton (`apps/desktop`, `apps/server`, `crates/*`)
- [x] Shared domain models, errors, and async trait contracts in `crates/common`
- [ ] Rune desktop UI design system, 6-section navigation shell, and `AppConfig` persistence

## Milestone 2 — Capture & Media Pipeline
- [ ] `crates/capture`: `gpu-screen-recorder` discovery (PATH & Flatpak fallback), replay buffer lifecycle, `SIGUSR1` save trigger, `SIGINT` stop, and `CaptureEvent` stream
- [ ] `crates/media`: `ffprobe` metadata extraction, thumbnail generation, and `ffmpeg` trimming/export

## Milestone 3 — Local Library & Desktop Integration
- [ ] `crates/library`: SQLite schema & migrations, `ClipStore` CRUD, search/game/tag filtering, directory watcher/importer
- [ ] Wire `apps/desktop` Library, Video Editor (timeline + trim), and Recording views to live backends

## Milestone 4 — Cloud Sharing & Web Viewer
- [ ] `apps/server`: Axum upload, range-streaming, and share-link API
- [ ] `crates/cloud-client`: `CloudClient` upload progress and share management
- [ ] `apps/web`: Web share viewer for shared clip links
