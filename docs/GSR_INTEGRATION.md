# GPU Screen Recorder (GSR) Integration

ClipForge uses [GPU Screen Recorder](https://git.dec05eba.com/gpu-screen-recorder/) (`gpu-screen-recorder`) as its primary capture and replay-buffer backend in `crates/capture`.

## Binary Discovery

At runtime, `crates/capture` checks for `gpu-screen-recorder` availability via:
1. Native `gpu-screen-recorder` binary in `PATH`
2. Flatpak fallback: `flatpak run com.dec05eba.gpu_screen_recorder`

If neither is present, `CaptureBackend::is_available()` returns `false` and start/record calls return `ClipForgeError::CaptureUnavailable` with installation guidance.

## Verified CLI Interface

Based on upstream `dec05eba/gpu-screen-recorder` documentation and `gpu-screen-recorder(1)`:

- **Capture Target (`-w`)**:
  - `screen` (primary monitor), `<monitor>` (e.g. `DP-1`), `focused` (X11 only, requires `-s WxH`), `portal` (Wayland `xdg-desktop-portal` + PipeWire), `region`, or `/dev/video*` (combinable with `|`).
  - Discovery flags: `--list-capture-options`, `--list-monitors`, `--list-v4l2-devices`.
- **Wayland Portal Persistence**:
  - `-restore-portal-session yes|no`
  - `-portal-session-token-filepath <path>` (default `~/.config/gpu-screen-recorder/restore_token`).
- **Output (`-o`)**:
  - Output directory for replay buffer mode (`-r`), or file path for standard recording.
- **Container & Codecs (`-c`, `-k`, `-ac`, `-ab`)**:
  - Container: `-c <mp4|mkv|flv|webm>`
  - Video codec: `-k <auto|h264|hevc|av1|vp8|vp9|hevchdr|av1hdr>`
  - Audio codec & bitrate: `-ac <opus|aac|flac>`, `-ab <kbps>`
- **Encoder & Quality**:
  - `-encoder <gpu|cpu>` (NVENC / VAAPI / Vulkan on GPU), `-fallback-cpu-encoding <yes|no>`
  - `-q <medium|high|veryhigh|ultra>`, `-bm <auto|qp|vbr|cbr>` (`cbr` recommended for replay), `-f <fps>`, `-fm <cfr|vfr|content>`, `-cr <limited|full>`, `-keyint <seconds>`.
- **Audio Sources (`-a`, repeatable)**:
  - `default_output`, `default_input`, `device:<name>`, `app:<name>` (PipeWire).
  - Discovery flags: `--list-audio-devices`, `--list-application-audio`.

## Replay Buffer, Signals & IPC

- **Replay Mode**: `-r <2..86400>` (seconds), `-replay-storage <ram|disk>` (default `ram`), `-restart-replay-on-save <yes|no>`.
- **POSIX Signals**:
  - `SIGUSR1`: Save current replay buffer to `-o` directory (replay mode only).
  - `SIGINT`: Stop & save (regular recording) or stop without saving (replay mode).
  - `SIGUSR2`: Pause/unpause (regular recording only).
- **Save Notification & IPC**:
  - `-sc <script_path>`: Async post-save hook receiving the saved file path as `$1` (GSR also prints the saved path to stdout).
  - `-ipc <unix_socket_path>`: Unix domain socket supported in modern GSR versions for programmatic control.
