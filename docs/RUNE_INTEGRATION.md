# Rune Desktop UI Integration

ClipForge's desktop application (`apps/desktop`) uses [Rune](https://github.com/anomalyco/rune) (`anomalyco/rune`), a native GPU-rendered Rust UI framework (not to be confused with the `rune-rs/rune` scripting language on crates.io).

## Dependency Pinning & Workspace Patches

`anomalyco/rune` is pre-release and developed on branch `dev`, so it is consumed as a Git dependency pinned to a specific commit `rev` in the root `Cargo.toml`:

- **Repository**: `https://github.com/anomalyco/rune.git`
- **Pinned `rev`**: `dc8f7f7213cbe43af45212de6998d530d0945187`
- **Rust Edition**: `2024`

Because `anomalyco/rune` patches six crates in its `vendor/` directory, the ClipForge root `Cargo.toml` mirrors `[patch.crates-io]` for:
- `wgpu-hal`
- `winit`
- `sctk-adwaita`
- `parley_engine`
- `accesskit_windows`
- `accesskit_macos`

Root `Cargo.toml` also sets `[profile.dev] debug = "line-tables-only"` and `[profile.dev.package."*"] debug = false` to avoid excessive debug-info memory usage.

## Linux Runtime & Headless Verification

- **Compile-time**: Rune uses pure-Rust / `dlopen`-based backends on Linux (`wgpu` Vulkan via `ash` + `libloading`, `winit` X11/Wayland via `dlopen`, `accesskit_unix` via `zbus` D-Bus), requiring no C `-dev` headers at build time.
- **Runtime**: Requires a working Vulkan ICD, X11 or Wayland display server, and D-Bus.
- **Headless Testing**: Debug builds enable Rune's `tooling` feature automatically, supporting:
  ```bash
  cargo run -p clipforge-desktop -- --snap /tmp/clipforge.png
  ```
  and `--tree` / `--script` inspection flags as well as `rune::Runtime` unit tests.
