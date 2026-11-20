# Security Policy

## Scope & Threat Model

ClipForge handles local screen/game recordings, user-configured paths, local SQLite metadata, and optional HTTP uploads to a self-hosted `clipforge-server`.

## Security Practices

- **Subprocess Invocation (`crates/capture`, `crates/media`)**:
  - Invoke `gpu-screen-recorder`, `ffmpeg`, and `ffprobe` using `Command::arg` / `args` arrays—never pass user-controlled strings through `sh -c`.
  - Validate trim ranges (`TrimRequest::validate()`) and sanitize file paths before writing outputs.
- **Network & Credentials (`crates/cloud-client`, `apps/server`)**:
  - Use `reqwest` with `rustls-tls` for HTTPS transport.
  - Store API tokens in user config with restrictive permissions; never commit tokens, `.env` files, or credentials to Git.
  - Validate upload sizes, content types, and share IDs on `apps/server` to prevent path traversal.
- **Wayland Portal Tokens**:
  - Keep `xdg-desktop-portal` restore tokens in user-private config directories (`~/.config/gpu-screen-recorder/`).

## Reporting a Vulnerability

Please report security issues privately to the maintainers rather than opening a public issue with exploit details.
