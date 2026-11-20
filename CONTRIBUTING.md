# Contributing to ClipForge

Thank you for your interest in contributing to ClipForge!

## Development Workflow

1. Fork and clone the repository.
2. Create a feature branch from `main`.
3. Make focused changes within the relevant crate (`crates/*` or `apps/*`).
4. Ensure formatting, compilation, and tests pass before opening a PR:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

## Code Style & Architecture Guidelines

- Follow `rustfmt.toml` (`edition = "2024"`, `max_width = 100`).
- Keep shared domain types and trait contracts in `crates/common` (`clipforge-common`).
- Keep UI rendering non-blocking: offload capture, FFmpeg, SQLite, and HTTP work to background Tokio tasks or channels.
- Do not hardcode user-specific paths or credentials; use `AppConfig` and XDG directories.
- Write unit tests for new domain logic, CLI argument builders, and database queries.

## Reporting Issues

When reporting bugs, please include:
- Distro & kernel version (`uname -a`, `/etc/os-release`)
- Display server (`X11` vs `Wayland` and desktop environment)
- GPU & driver (`lspci`, `vulkaninfo --summary`)
- `gpu-screen-recorder` and `ffmpeg` versions
