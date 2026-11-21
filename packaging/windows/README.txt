ClipForge - Windows x64 Portable
================================

ClipForge is an open-source game clipping, replay-buffer, and clip-sharing
desktop application.

Quick Start
-----------
1. Extract all files from this ZIP archive into a folder of your choice
   (no installer or administrator privileges are required).
2. Double-click `ClipForge.exe` (or run it from PowerShell / Command Prompt)
   to launch ClipForge.
   - Note: If Windows SmartScreen prompts on an unsigned local/CI build,
     click "More info" -> "Run anyway".

Requirements & Bundled Tools
--------------------------
- FFmpeg (`ffmpeg.exe` and `ffprobe.exe`):
  Required for clip trimming, media playback decoding, and thumbnail
  generation. `ffmpeg.exe` and `ffprobe.exe` are bundled in this portable
  archive alongside `ClipForge.exe`, so no separate FFmpeg installation is
  required. See `THIRD-PARTY-NOTICES.txt` for FFmpeg license terms and source
  information.

Known Windows Limitations
-------------------------
- Screen & Game Recording:
  ClipForge's primary recording engine (`gpu-screen-recorder`) is Linux-only.
  Native Windows capture (Windows Graphics Capture / WASAPI) is not yet
  implemented or requires validation on Windows hardware.
- Linux-Only Desktop Integrations:
  Linux-specific features such as XDG GlobalShortcuts (`zbus`), PulseAudio/
  PipeWire mic testing (`pactl`/`parec`), and GNOME Keyring (`secret-tool`)
  are unavailable on Windows.

Troubleshooting & Diagnostics
-----------------------------
- Use Settings -> About -> Export Diagnostics inside ClipForge to generate a
  sanitized diagnostic report (application version, OS/arch, dependency
  status, and redacted configuration). API tokens, passwords, and private
  clip contents are never included.

License
-------
ClipForge is licensed under the MIT License. See `LICENSE` in this directory.
