//! Recording Studio view: live GSR status, replay/record controls, capture pipeline, audio, and hotkeys.

use chrono::{DateTime, Utc};
use clipforge_common::{
    AudioCodec, CaptureStatus, CaptureTarget, ContainerFormat, EncoderMode, QualityPreset,
    VideoCodec,
};
use rune::{Cursor, El, ScrollSpec, col, row, text};

use crate::components::{
    badge, btn_danger, btn_primary, btn_secondary, card, checkbox, error_state, kv_row,
    pill_option, progress_bar, section_header, segmented_control, segmented_item, status_pill,
    toggle_switch,
};
use crate::state::AppState;
use crate::theme::{Palette, radius, space, typography};

/// Renders the Obsidian Forge Recording Studio view bound to real GSR and `AppState` data.
pub fn view(p: &Palette, s: &AppState) -> El {
    let mut root = col()
        .key("recording-view")
        .w_full()
        .gap(space::LG)
        .child(live_status_banner(p, s));

    if let CaptureStatus::Error { message } = &s.capture_status {
        root = root.child(capture_error_banner(p, message));
    }

    if s.deps.probed && !s.deps.gsr_available {
        root = root.child(missing_gsr_banner(p));
    }

    root.child(
        row()
            .key("rec-top-row")
            .w_full()
            .gap(space::LG)
            .child(replay_hero_card(p, s).grow(1.5).min_w(0.0))
            .child(capture_health_card(p, s).grow(1.0).min_w(0.0)),
    )
    .child(
        row()
            .key("rec-bottom-row")
            .w_full()
            .gap(space::LG)
            .child(capture_pipeline_card(p, s).grow(1.5).min_w(0.0))
            .child(
                col()
                    .key("rec-right-col")
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(space::LG)
                    .child(audio_sources_card(p, s))
                    .child(hotkeys_card(p, s)),
            ),
    )
}

fn live_status_banner(p: &Palette, s: &AppState) -> El {
    let cfg = &s.config.capture;
    let session_label = if s.is_wayland { "Wayland" } else { "X11" };
    let target_desc = target_label(&cfg.capture_target);

    let (dot, label_text, fg, bg, hk) = match &s.capture_status {
        CaptureStatus::ReplayBuffering { buffer_secs, .. } => (
            p.success,
            format!("REPLAY BUFFER RUNNING ({buffer_secs}s)"),
            p.success_text,
            p.success_soft,
            Some(s.hotkeys.save_replay.as_str()),
        ),
        CaptureStatus::Recording { paused, .. } => (
            p.recording,
            if *paused {
                "RECORDING PAUSED".to_string()
            } else {
                "RECORDING LIVE".to_string()
            },
            p.recording,
            p.recording_soft,
            Some(s.hotkeys.toggle_record.as_str()),
        ),
        CaptureStatus::Error { .. } => (
            p.recording,
            "CAPTURE ERROR".to_string(),
            p.error_text,
            p.recording_soft,
            None,
        ),
        CaptureStatus::Idle => (
            p.text_muted,
            "IDLE • STANDBY".to_string(),
            p.text_secondary,
            p.bg_surface,
            Some(s.hotkeys.toggle_replay.as_str()),
        ),
    };

    row()
        .key("rec-live-banner")
        .w_full()
        .px(space::MD)
        .py(space::SM)
        .justify_between()
        .center_y()
        .fill(p.bg_lowest)
        .border(1.0, p.border_subtle)
        .radius(radius::MD)
        .child(
            row()
                .key("rec-live-left")
                .gap(space::MD)
                .center_y()
                .child(status_pill(
                    p,
                    "rec-live-pill",
                    dot,
                    &label_text,
                    fg,
                    bg,
                    hk,
                ))
                .child(
                    text(format!(
                        "GPU Screen Recorder  •  Target: {target_desc}  •  Session: {session_label}"
                    ))
                    .font(typography::MONO_BADGE)
                    .color(p.text_secondary),
                ),
        )
        .child(
            row()
                .key("rec-live-right")
                .gap(space::SM)
                .center_y()
                .child(badge(
                    "rec-fps-badge",
                    &format!("{} FPS", cfg.fps),
                    p.accent_primary,
                    p.accent_soft,
                ))
                .child(badge(
                    "rec-codec-badge",
                    &format!(
                        "{} • {}",
                        video_codec_label(cfg.video_codec),
                        encoder_short_label(cfg.encoder)
                    ),
                    p.text_secondary,
                    p.bg_surface,
                )),
        )
}

fn capture_error_banner(p: &Palette, message: &str) -> El {
    let actions = row()
        .key("rec-err-actions")
        .gap(space::SM)
        .center_y()
        .child(btn_primary(
            p,
            "rec-err-restart",
            "Restart Replay Buffer",
            |st| {
                st.clear_capture_error();
                st.toggle_replay_buffer();
            },
        ))
        .child(btn_secondary(p, "rec-clear-err", "Clear Error", |st| {
            st.clear_capture_error();
        }));

    error_state(
        p,
        "rec-error-banner",
        "Capture Backend Error",
        message,
        Some(actions),
    )
}

fn missing_gsr_banner(p: &Palette) -> El {
    row()
        .key("rec-missing-gsr")
        .w_full()
        .px(space::LG)
        .py(space::MD)
        .justify_between()
        .center_y()
        .fill(p.warning_soft)
        .border(1.0, p.warning)
        .radius(radius::MD)
        .child(
            col()
                .key("rec-missing-copy")
                .gap(2.0)
                .child(
                    text("gpu-screen-recorder not detected on PATH or Flatpak")
                        .font(typography::LABEL)
                        .color(p.warning_text),
                )
                .child(
                    text(
                        "Install gpu-screen-recorder to enable live replay buffering and capture.",
                    )
                    .font(typography::BODY)
                    .color(p.text_secondary),
                ),
        )
        .child(btn_secondary(
            p,
            "rec-reprobe-gsr",
            "Refresh Sources",
            |st| st.discover_capture_sources(),
        ))
}

fn replay_hero_card(p: &Palette, s: &AppState) -> El {
    let cfg = &s.config.capture;
    let is_buffering = matches!(s.capture_status, CaptureStatus::ReplayBuffering { .. });
    let is_recording = matches!(s.capture_status, CaptureStatus::Recording { .. });
    let is_error = matches!(s.capture_status, CaptureStatus::Error { .. });

    let (state_badge_text, state_fg, state_bg) = if is_recording {
        ("RECORDING", p.recording, p.recording_soft)
    } else if is_buffering {
        ("ARMED", p.success_text, p.success_soft)
    } else if is_error {
        ("ERROR", p.error_text, p.recording_soft)
    } else {
        ("IDLE", p.text_muted, p.bg_lowest)
    };

    let subtitle = match &s.capture_status {
        CaptureStatus::ReplayBuffering {
            started_at,
            buffer_secs,
        } => format!(
            "Active {buffer_secs}s rolling buffer • Running for {}",
            format_elapsed(*started_at)
        ),
        CaptureStatus::Recording {
            started_at,
            output_path,
            paused,
        } => {
            let file = output_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("recording");
            let state_word = if *paused { "Paused" } else { "Recording" };
            format!(
                "{state_word} to {file} • Elapsed {}",
                format_elapsed(*started_at)
            )
        }
        CaptureStatus::Error { .. } => {
            "Capture halted due to backend error — clear error or restart".to_string()
        }
        CaptureStatus::Idle => format!(
            "Buffer idle on {} — ready to start replay buffer or direct recording",
            target_label(&cfg.capture_target)
        ),
    };

    let (bar_fill, bar_color) = if is_recording {
        (1.0, p.recording)
    } else if is_buffering {
        (1.0, p.success)
    } else {
        (0.0, p.bg_active)
    };

    let replay_toggle_btn = if is_buffering {
        btn_danger(p, "rec-replay-btn", "Stop Replay Buffer", |st| {
            st.toggle_replay_buffer();
        })
    } else {
        btn_secondary(
            p,
            "rec-replay-btn",
            if is_error {
                "Restart Replay Buffer"
            } else {
                "Start Replay Buffer"
            },
            |st| st.toggle_replay_buffer(),
        )
    };

    let record_btn = if is_recording {
        btn_danger(
            p,
            "rec-record-btn",
            &format!("Stop Recording ({})", s.hotkeys.toggle_record),
            |st| st.toggle_recording(),
        )
    } else {
        btn_secondary(
            p,
            "rec-record-btn",
            &format!("Start Recording ({})", s.hotkeys.toggle_record),
            |st| st.toggle_recording(),
        )
    };

    card(p, "rec-controls-card")
        .justify_between()
        .child(
            row()
                .key("rec-hdr-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    col()
                        .key("rec-hero-meta")
                        .gap(4.0)
                        .child(
                            row()
                                .key("rec-hero-title-row")
                                .gap(space::SM)
                                .center_y()
                                .child(
                                    text("Rolling Replay Buffer")
                                        .font(typography::HEADLINE_SM)
                                        .color(p.text_primary),
                                )
                                .child(badge(
                                    "rec-hero-state-badge",
                                    state_badge_text,
                                    state_fg,
                                    state_bg,
                                )),
                        )
                        .child(
                            text(subtitle)
                                .font(typography::LABEL)
                                .color(p.text_secondary),
                        ),
                )
                .child(replay_toggle_btn),
        )
        .child(
            row()
                .key("rec-btn-row")
                .w_full()
                .gap(space::MD)
                .child(
                    btn_primary(
                        p,
                        "rec-save-replay-btn",
                        &format!(
                            "Save Last {}s Replay ({})",
                            cfg.replay_buffer_secs, s.hotkeys.save_replay
                        ),
                        |st| st.trigger_save_replay(),
                    )
                    .grow(1.0),
                )
                .child(record_btn),
        )
        .child(
            col()
                .key("rec-hero-footer")
                .w_full()
                .gap(space::XS)
                .child(
                    row()
                        .key("rec-hero-telemetry")
                        .w_full()
                        .justify_between()
                        .center_y()
                        .child(
                            text(format!(
                                "Buffer Window: {}s  •  Output: {}",
                                cfg.replay_buffer_secs,
                                cfg.output_dir.display()
                            ))
                            .font(typography::MONO_BADGE)
                            .color(if is_buffering {
                                p.success_text
                            } else {
                                p.text_muted
                            }),
                        )
                        .child(
                            text(format!(
                                "{} FPS  •  {}",
                                cfg.fps,
                                target_label(&cfg.capture_target)
                            ))
                            .font(typography::MONO_BADGE)
                            .color(p.text_secondary),
                        ),
                )
                .child(progress_bar(p, "rec-buffer-bar", bar_fill, bar_color)),
        )
}

fn capture_health_card(p: &Palette, s: &AppState) -> El {
    let cfg = &s.config.capture;
    let is_error = matches!(s.capture_status, CaptureStatus::Error { .. });

    let (health_label, health_fg, health_bg) = if is_error {
        ("Error", p.error_text, p.recording_soft)
    } else if s.deps.gsr_available {
        ("Ready", p.success_text, p.success_soft)
    } else {
        ("GSR Missing", p.warning_text, p.warning_soft)
    };

    let gsr_val = s.deps.gsr_location.clone().unwrap_or_else(|| {
        if s.deps.gsr_available {
            "Available".to_string()
        } else {
            "Not found on PATH".to_string()
        }
    });

    let media_tools_val = format!(
        "{} / {}",
        if s.deps.ffmpeg_available {
            "FFmpeg OK"
        } else {
            "FFmpeg Missing"
        },
        if s.deps.ffprobe_available {
            "Probe OK"
        } else {
            "Probe Missing"
        }
    );

    card(p, "rec-health-card")
        .gap(space::SM)
        .child(
            row()
                .key("rec-health-hdr")
                .w_full()
                .justify_between()
                .center_y()
                .child(section_header(
                    p,
                    "rec-health-title",
                    "Capture Health",
                    "Live backend & dependency state",
                ))
                .child(badge(
                    "rec-health-badge",
                    health_label,
                    health_fg,
                    health_bg,
                )),
        )
        .child(kv_row(p, "rec-hlth-gsr", "GSR Engine", gsr_val))
        .child(kv_row(p, "rec-hlth-media", "Media Tools", media_tools_val))
        .child(kv_row(
            p,
            "rec-hlth-enc",
            "Encoder & Codec",
            format!(
                "{} • {}",
                encoder_short_label(cfg.encoder),
                video_codec_label(cfg.video_codec)
            ),
        ))
        .child(kv_row(
            p,
            "rec-hlth-src",
            "Capture Source",
            target_label(&cfg.capture_target),
        ))
        .child(kv_row(
            p,
            "rec-hlth-session",
            "Display Session",
            if s.is_wayland {
                "Wayland".to_string()
            } else {
                "X11".to_string()
            },
        ))
        .child(kv_row(
            p,
            "rec-hlth-out",
            "Output Dir",
            cfg.output_dir.display().to_string(),
        ))
}

fn capture_pipeline_card(p: &Palette, s: &AppState) -> El {
    let cfg = &s.config.capture;
    let is_region = matches!(cfg.capture_target, CaptureTarget::Region { .. });

    let mut card_el = card(p, "rec-config-card")
        .child(
            row()
                .key("rec-cfg-hdr-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(section_header(
                    p,
                    "rec-cfg-hdr",
                    "Capture Engine Pipeline",
                    "Applied when starting gpu-screen-recorder",
                ))
                .child(btn_secondary(
                    p,
                    "rec-refresh-sources",
                    "Refresh Sources",
                    |st| st.discover_capture_sources(),
                )),
        )
        .child(target_selector_section(p, s, is_region))
        .child(monitors_section(p, s));

    if cfg.capture_target == CaptureTarget::Portal {
        let restore_portal = cfg.restore_portal_session;
        card_el = card_el.child(
            row()
                .key("cfg-portal-restore-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    col()
                        .key("portal-restore-meta")
                        .gap(2.0)
                        .child(
                            text("Restore Portal Session")
                                .font(typography::BODY)
                                .color(p.text_primary),
                        )
                        .child(
                            text("Reuse Wayland screencast permission token across restarts")
                                .font(typography::LABEL)
                                .color(p.text_muted),
                        ),
                )
                .child(toggle_switch(
                    p,
                    "cfg-restore-portal-switch",
                    "Restore Portal Session",
                    restore_portal,
                    move |st| st.set_restore_portal_session(!restore_portal),
                )),
        );
    }

    if let CaptureTarget::Region {
        width,
        height,
        x,
        y,
    } = cfg.capture_target
    {
        card_el = card_el.child(
            row()
                .key("cfg-region-row")
                .w_full()
                .justify_between()
                .center_y()
                .px(space::SM)
                .py(space::XS)
                .fill(p.bg_lowest)
                .border(1.0, p.border_subtle)
                .radius(radius::SM)
                .child(
                    text(format!("Region: {width}×{height} at ({x}, {y})"))
                        .font(typography::MONO_METRIC)
                        .color(p.text_primary),
                )
                .child(
                    row()
                        .key("cfg-region-presets")
                        .gap(space::XS)
                        .child(pill_option(
                            p,
                            "region-720p",
                            "1280×720",
                            width == 1280 && height == 720,
                            |st| {
                                st.set_capture_target(CaptureTarget::Region {
                                    width: 1280,
                                    height: 720,
                                    x: 0,
                                    y: 0,
                                });
                            },
                        ))
                        .child(pill_option(
                            p,
                            "region-1080p",
                            "1920×1080",
                            width == 1920 && height == 1080,
                            |st| {
                                st.set_capture_target(CaptureTarget::Region {
                                    width: 1920,
                                    height: 1080,
                                    x: 0,
                                    y: 0,
                                });
                            },
                        )),
                ),
        );
    }

    let restart_on_save = cfg.restart_replay_on_save;

    card_el = card_el
        .child(resolution_row(p, cfg.width, cfg.height))
        .child(fps_row(p, cfg.fps))
        .child(video_codec_row(p, cfg.video_codec));

    if cfg.video_codec == VideoCodec::Av1 {
        card_el = card_el.child(fallback_notice(
            p,
            "rec-av1-note",
            "AV1 fallback note: GPUs without hardware AV1 encoding (e.g. RTX 30-series) will fall back to H.264/HEVC or CPU encoding in GSR.",
        ));
    }

    card_el
        .child(encoder_and_quality_row(p, cfg.encoder, cfg.quality))
        .child(container_and_buffer_row(
            p,
            cfg.container,
            cfg.replay_buffer_secs,
        ))
        .child(
            row()
                .key("cfg-restart-on-save-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    col()
                        .key("restart-save-meta")
                        .gap(2.0)
                        .child(
                            text("Restart Replay Buffer After Save")
                                .font(typography::BODY)
                                .color(p.text_primary),
                        )
                        .child(
                            text("Clears and restarts the rolling buffer after saving a replay")
                                .font(typography::LABEL)
                                .color(p.text_muted),
                        ),
                )
                .child(toggle_switch(
                    p,
                    "cfg-restart-save-switch",
                    "Restart Replay Buffer After Save",
                    restart_on_save,
                    move |st| st.set_restart_replay_on_save(!restart_on_save),
                )),
        )
}

fn target_selector_section(p: &Palette, s: &AppState, is_region: bool) -> El {
    let target = &s.config.capture.capture_target;

    let focused_btn = if s.is_wayland {
        row()
            .key("target-focused")
            .role("button")
            .label("Focused Window (X11 only)")
            .cursor(Cursor::Default)
            .px(space::SM)
            .h(26.0)
            .center()
            .radius(radius::XS)
            .fill(p.bg_surface)
            .child(
                text("Focused Window (X11)")
                    .font(typography::LABEL)
                    .color(p.text_disabled)
                    .a11y_hidden(),
            )
    } else {
        segmented_item(
            p,
            "target-focused",
            "Focused Window",
            *target == CaptureTarget::FocusedWindow,
            |st| st.set_capture_target(CaptureTarget::FocusedWindow),
        )
    };

    let mut section = col().key("cfg-target-col").w_full().gap(space::XS).child(
        row()
            .key("cfg-target-row")
            .w_full()
            .justify_between()
            .center_y()
            .child(
                text("Capture Target")
                    .font(typography::BODY)
                    .color(p.text_secondary),
            )
            .child(segmented_control(
                p,
                "target-opts",
                vec![
                    segmented_item(
                        p,
                        "target-screen",
                        "Primary Screen",
                        *target == CaptureTarget::PrimaryScreen,
                        |st| st.set_capture_target(CaptureTarget::PrimaryScreen),
                    ),
                    focused_btn,
                    segmented_item(
                        p,
                        "target-portal",
                        "Wayland Portal",
                        *target == CaptureTarget::Portal,
                        |st| st.set_capture_target(CaptureTarget::Portal),
                    ),
                    segmented_item(p, "target-region", "Region", is_region, |st| {
                        if !matches!(
                            st.config.capture.capture_target,
                            CaptureTarget::Region { .. }
                        ) {
                            st.set_capture_target(CaptureTarget::Region {
                                width: 1280,
                                height: 720,
                                x: 0,
                                y: 0,
                            });
                        }
                    }),
                ],
            )),
    );

    if s.is_wayland {
        section = section.child(fallback_notice(
            p,
            "rec-wayland-focused-warn",
            "Focused Window capture is X11-only and disabled on Wayland. Use Primary Screen, a specific Monitor, Region, or Wayland Portal.",
        ));
    }

    section
}

fn monitors_section(p: &Palette, s: &AppState) -> El {
    let active_monitor = match &s.config.capture.capture_target {
        CaptureTarget::Monitor(name) => Some(name.as_str()),
        _ => None,
    };

    if s.available_monitors.is_empty() && active_monitor.is_none() {
        return row()
            .key("cfg-monitors-empty")
            .w_full()
            .justify_between()
            .center_y()
            .child(
                text("Monitors")
                    .font(typography::BODY)
                    .color(p.text_secondary),
            )
            .child(
                text("Click Refresh Sources to enumerate displays from GSR")
                    .font(typography::LABEL)
                    .color(p.text_muted),
            );
    }

    let mut mon_row = row().key("monitor-opts").gap(space::SM);
    for (idx, mon) in s.available_monitors.iter().enumerate() {
        let name = mon.name.clone();
        let label = match &mon.resolution {
            Some(res) => format!("{} ({res})", mon.name),
            None => mon.name.clone(),
        };
        let selected = active_monitor == Some(mon.name.as_str());
        mon_row = mon_row.child(pill_option(
            p,
            &format!("mon-{idx}-{}", mon.name),
            &label,
            selected,
            move |st| st.set_capture_target(CaptureTarget::Monitor(name.clone())),
        ));
    }

    if let Some(active_name) = active_monitor
        && !s.available_monitors.iter().any(|m| m.name == active_name)
    {
        let owned = active_name.to_string();
        mon_row = mon_row.child(pill_option(
            p,
            "mon-active-custom",
            active_name,
            true,
            move |st| st.set_capture_target(CaptureTarget::Monitor(owned.clone())),
        ));
    }

    row()
        .key("cfg-monitors-row")
        .w_full()
        .justify_between()
        .center_y()
        .child(
            text("Target Monitor")
                .font(typography::BODY)
                .color(p.text_secondary),
        )
        .child(mon_row)
}

fn resolution_row(p: &Palette, width: Option<u32>, height: Option<u32>) -> El {
    row()
        .key("cfg-res-row")
        .w_full()
        .justify_between()
        .center_y()
        .child(
            text("Output Resolution")
                .font(typography::BODY)
                .color(p.text_secondary),
        )
        .child(
            row()
                .key("res-opts")
                .gap(space::SM)
                .child(pill_option(
                    p,
                    "res-native",
                    "Native",
                    width.is_none() && height.is_none(),
                    |st| st.set_capture_resolution(None, None),
                ))
                .child(pill_option(
                    p,
                    "res-720p",
                    "720p",
                    width == Some(1280) && height == Some(720),
                    |st| st.set_capture_resolution(Some(1280), Some(720)),
                ))
                .child(pill_option(
                    p,
                    "res-1080p",
                    "1080p",
                    width == Some(1920) && height == Some(1080),
                    |st| st.set_capture_resolution(Some(1920), Some(1080)),
                ))
                .child(pill_option(
                    p,
                    "res-1440p",
                    "1440p",
                    width == Some(2560) && height == Some(1440),
                    |st| st.set_capture_resolution(Some(2560), Some(1440)),
                )),
        )
}

fn fps_row(p: &Palette, fps: u32) -> El {
    row()
        .key("cfg-fps-row")
        .w_full()
        .justify_between()
        .center_y()
        .child(
            text("Frame Rate")
                .font(typography::BODY)
                .color(p.text_secondary),
        )
        .child(
            row()
                .key("fps-opts")
                .gap(space::SM)
                .child(pill_option(p, "fps-30", "30 FPS", fps == 30, |st| {
                    st.set_capture_fps(30);
                }))
                .child(pill_option(p, "fps-60", "60 FPS", fps == 60, |st| {
                    st.set_capture_fps(60);
                }))
                .child(pill_option(p, "fps-120", "120 FPS", fps == 120, |st| {
                    st.set_capture_fps(120);
                })),
        )
}

fn video_codec_row(p: &Palette, codec: VideoCodec) -> El {
    row()
        .key("cfg-vcodec-row")
        .w_full()
        .justify_between()
        .center_y()
        .child(
            text("Video Codec")
                .font(typography::BODY)
                .color(p.text_secondary),
        )
        .child(
            row()
                .key("vcodec-opts")
                .gap(space::SM)
                .child(pill_option(
                    p,
                    "vc-auto",
                    "Auto",
                    codec == VideoCodec::Auto,
                    |st| st.set_video_codec(VideoCodec::Auto),
                ))
                .child(pill_option(
                    p,
                    "vc-h264",
                    "H.264",
                    codec == VideoCodec::H264,
                    |st| st.set_video_codec(VideoCodec::H264),
                ))
                .child(pill_option(
                    p,
                    "vc-hevc",
                    "HEVC",
                    codec == VideoCodec::Hevc,
                    |st| st.set_video_codec(VideoCodec::Hevc),
                ))
                .child(pill_option(
                    p,
                    "vc-av1",
                    "AV1",
                    codec == VideoCodec::Av1,
                    |st| st.set_video_codec(VideoCodec::Av1),
                )),
        )
}

fn encoder_and_quality_row(p: &Palette, encoder: EncoderMode, quality: QualityPreset) -> El {
    col()
        .key("cfg-enc-qual-col")
        .w_full()
        .gap(space::MD)
        .child(
            row()
                .key("cfg-enc-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    text("Encoder Mode")
                        .font(typography::BODY)
                        .color(p.text_secondary),
                )
                .child(
                    row()
                        .key("enc-opts")
                        .gap(space::SM)
                        .child(pill_option(
                            p,
                            "enc-gpu",
                            "GPU (NVENC/VAAPI)",
                            encoder == EncoderMode::Gpu,
                            |st| st.set_encoder(EncoderMode::Gpu),
                        ))
                        .child(pill_option(
                            p,
                            "enc-cpu",
                            "CPU",
                            encoder == EncoderMode::Cpu,
                            |st| st.set_encoder(EncoderMode::Cpu),
                        )),
                ),
        )
        .child(
            row()
                .key("cfg-qual-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    text("Quality Preset")
                        .font(typography::BODY)
                        .color(p.text_secondary),
                )
                .child(
                    row()
                        .key("qual-opts")
                        .gap(space::SM)
                        .child(pill_option(
                            p,
                            "q-med",
                            "Medium",
                            quality == QualityPreset::Medium,
                            |st| st.set_quality(QualityPreset::Medium),
                        ))
                        .child(pill_option(
                            p,
                            "q-high",
                            "High",
                            quality == QualityPreset::High,
                            |st| st.set_quality(QualityPreset::High),
                        ))
                        .child(pill_option(
                            p,
                            "q-vhigh",
                            "Very High",
                            quality == QualityPreset::VeryHigh,
                            |st| st.set_quality(QualityPreset::VeryHigh),
                        ))
                        .child(pill_option(
                            p,
                            "q-ultra",
                            "Ultra",
                            quality == QualityPreset::Ultra,
                            |st| st.set_quality(QualityPreset::Ultra),
                        )),
                ),
        )
}

fn container_and_buffer_row(p: &Palette, container: ContainerFormat, buffer_secs: u32) -> El {
    col()
        .key("cfg-cont-buf-col")
        .w_full()
        .gap(space::MD)
        .child(
            row()
                .key("cfg-cont-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    text("Container Format")
                        .font(typography::BODY)
                        .color(p.text_secondary),
                )
                .child(
                    row()
                        .key("cont-opts")
                        .gap(space::SM)
                        .child(pill_option(
                            p,
                            "cont-mp4",
                            "MP4",
                            container == ContainerFormat::Mp4,
                            |st| st.set_container(ContainerFormat::Mp4),
                        ))
                        .child(pill_option(
                            p,
                            "cont-mkv",
                            "MKV",
                            container == ContainerFormat::Mkv,
                            |st| st.set_container(ContainerFormat::Mkv),
                        ))
                        .child(pill_option(
                            p,
                            "cont-webm",
                            "WebM",
                            container == ContainerFormat::Webm,
                            |st| st.set_container(ContainerFormat::Webm),
                        )),
                ),
        )
        .child(
            row()
                .key("cfg-buf-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    text("Replay Buffer Duration")
                        .font(typography::BODY)
                        .color(p.text_secondary),
                )
                .child(
                    row()
                        .key("buf-opts")
                        .gap(space::SM)
                        .child(pill_option(p, "buf-30", "30s", buffer_secs == 30, |st| {
                            st.set_replay_buffer_secs(30);
                        }))
                        .child(pill_option(p, "buf-60", "60s", buffer_secs == 60, |st| {
                            st.set_replay_buffer_secs(60);
                        }))
                        .child(pill_option(
                            p,
                            "buf-120",
                            "120s",
                            buffer_secs == 120,
                            |st| st.set_replay_buffer_secs(120),
                        ))
                        .child(pill_option(p, "buf-300", "5m", buffer_secs == 300, |st| {
                            st.set_replay_buffer_secs(300)
                        })),
                ),
        )
}

fn audio_sources_card(p: &Palette, s: &AppState) -> El {
    let cfg = &s.config.capture;
    let mut c = card(p, "rec-audio-card")
        .child(
            row()
                .key("rec-audio-hdr-row")
                .w_full()
                .min_w(0.0)
                .gap(space::SM)
                .justify_between()
                .center_y()
                .child(
                    section_header(
                        p,
                        "rec-audio-hdr",
                        "Audio Sources & Codec",
                        "Multi-track PipeWire / PulseAudio routing",
                    )
                    .grow(1.0)
                    .min_w(0.0),
                )
                .child(btn_secondary(p, "rec-audio-refresh", "Refresh", |st| {
                    st.discover_capture_sources();
                })),
        )
        .child(
            row()
                .key("cfg-acodec-row")
                .w_full()
                .min_w(0.0)
                .gap(space::SM)
                .justify_between()
                .center_y()
                .child(
                    text("Audio Codec")
                        .font(typography::BODY)
                        .color(p.text_secondary),
                )
                .child(
                    row()
                        .key("acodec-opts")
                        .gap(space::SM)
                        .child(pill_option(
                            p,
                            "ac-opus",
                            "Opus",
                            cfg.audio_codec == AudioCodec::Opus,
                            |st| st.set_audio_codec(AudioCodec::Opus),
                        ))
                        .child(pill_option(
                            p,
                            "ac-aac",
                            "AAC",
                            cfg.audio_codec == AudioCodec::Aac,
                            |st| st.set_audio_codec(AudioCodec::Aac),
                        ))
                        .child(pill_option(
                            p,
                            "ac-flac",
                            "FLAC",
                            cfg.audio_codec == AudioCodec::Flac,
                            |st| st.set_audio_codec(AudioCodec::Flac),
                        )),
                ),
        );

    if cfg.audio_codec == AudioCodec::Flac {
        c = c.child(fallback_notice(
            p,
            "rec-flac-note",
            "FLAC fallback note: MP4 containers have limited FLAC support; GSR may fall back to Opus/AAC or require MKV.",
        ));
    }

    c = c
        .child(audio_source_row(
            p,
            "audio-default-out",
            "default_output",
            "System Desktop Audio",
            cfg.audio_sources.iter().any(|src| src == "default_output"),
        ))
        .child(audio_source_row(
            p,
            "audio-default-in",
            "default_input",
            "Default Microphone",
            cfg.audio_sources.iter().any(|src| src == "default_input"),
        ));

    let has_discovered = !s.available_audio_devices.is_empty() || !s.available_app_audio.is_empty();
    if has_discovered {
        let mut list = col()
            .key("rec-audio-list")
            .w_full()
            .min_w(0.0)
            .min_h(0.0)
            .max_h(260.0)
            .gap(space::XS)
            .clip_rounded(radius::SM)
            .scroll(ScrollSpec {
                stick_bottom: false,
            });

        let extra_devices: Vec<_> = s
            .available_audio_devices
            .iter()
            .enumerate()
            .filter(|(_, dev)| dev.id != "default_output" && dev.id != "default_input")
            .collect();

        if !extra_devices.is_empty() {
            list = list.child(
                text("Discovered Audio Devices")
                    .font(typography::MONO_BADGE)
                    .color(p.text_muted),
            );
            for (idx, dev) in extra_devices {
                let active = cfg.audio_sources.iter().any(|src| src == &dev.id);
                list = list.child(audio_source_row(
                    p,
                    &format!("audio-dev-{idx}"),
                    &dev.id,
                    &dev.label,
                    active,
                ));
            }
        }

        if !s.available_app_audio.is_empty() {
            list = list.child(
                text("Application Audio Streams")
                    .font(typography::MONO_BADGE)
                    .color(p.text_muted),
            );
            for (idx, app) in s.available_app_audio.iter().enumerate() {
                let active = cfg.audio_sources.iter().any(|src| src == &app.id);
                list = list.child(audio_source_row(
                    p,
                    &format!("audio-app-{idx}"),
                    &app.id,
                    &app.label,
                    active,
                ));
            }
        }

        c = c.child(list);
    } else {
        c = c.child(
            text("Click Refresh to discover additional audio devices and per-app streams.")
                .font(typography::LABEL)
                .color(p.text_muted)
                .w_full()
                .wrap(),
        );
    }

    if !cfg.audio_sources.is_empty() {
        let mut inspector = col()
            .key("rec-audio-inspector")
            .w_full()
            .min_w(0.0)
            .gap(space::XS)
            .px(space::SM)
            .py(space::XS)
            .fill(p.bg_lowest)
            .border(1.0, p.border_subtle)
            .radius(radius::SM)
            .clip_rounded(radius::SM)
            .child(
                text("ACTIVE ROUTING DETAIL")
                    .font(typography::MONO_BADGE)
                    .color(p.text_muted),
            );

        for (idx, src_id) in cfg.audio_sources.iter().enumerate() {
            let full_label = resolve_audio_source_label(s, src_id);
            inspector = inspector.child(
                col()
                    .key(format!("rec-audio-active-{idx}"))
                    .w_full()
                    .min_w(0.0)
                    .gap(2.0)
                    .child(
                        text(full_label)
                            .w_full()
                            .font(typography::LABEL)
                            .color(p.text_primary)
                            .wrap(),
                    )
                    .child(
                        text(src_id.clone())
                            .w_full()
                            .font(typography::MONO_BADGE)
                            .color(p.text_muted)
                            .wrap_anywhere(),
                    ),
            );
        }

        c = c.child(inspector);
    }

    c
}

fn resolve_audio_source_label(s: &AppState, id: &str) -> String {
    if id == "default_output" {
        return "System Desktop Audio".to_string();
    }
    if id == "default_input" {
        return "Default Microphone".to_string();
    }
    if let Some(dev) = s.available_audio_devices.iter().find(|d| d.id == id) {
        return dev.label.clone();
    }
    if let Some(app) = s.available_app_audio.iter().find(|a| a.id == id) {
        return app.label.clone();
    }
    id.to_string()
}

fn audio_source_row(p: &Palette, key: &str, id: &str, label: &str, checked: bool) -> El {
    let source_id = id.to_string();
    row()
        .key(key)
        .w_full()
        .min_w(0.0)
        .px(space::SM)
        .py(space::SM)
        .gap(space::SM)
        .center_y()
        .fill(if checked { p.accent_soft } else { p.bg_lowest })
        .border(
            1.0,
            if checked {
                p.border_active
            } else {
                p.border_subtle
            },
        )
        .radius(radius::SM)
        .clip_rounded(radius::SM)
        .child(
            checkbox(p, &format!("{key}-chk"), label, checked, move |st| {
                st.toggle_audio_source(source_id.clone());
            })
            .min_w(16.0),
        )
        .child(
            col()
                .key(format!("{key}-info"))
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(
                    text(label.to_string())
                        .key(format!("{key}-label"))
                        .w_full()
                        .font(typography::LABEL)
                        .color(if checked {
                            p.text_primary
                        } else {
                            p.text_secondary
                        })
                        .truncate(),
                )
                .child(
                    text(id.to_string())
                        .key(format!("{key}-id"))
                        .w_full()
                        .font(typography::MONO_BADGE)
                        .color(p.text_muted)
                        .truncate_middle(),
                ),
        )
}

fn hotkeys_card(p: &Palette, s: &AppState) -> El {
    let subtitle = if s.is_wayland {
        "Wayland: configure GlobalShortcuts in compositor"
    } else {
        "X11 global keybindings"
    };

    card(p, "rec-hotkey-card")
        .child(section_header(p, "rec-hk-hdr", "Global Hotkeys", subtitle))
        .child(hotkey_row(
            p,
            "hk-save",
            "Save Replay Buffer",
            &s.hotkeys.save_replay,
        ))
        .child(hotkey_row(
            p,
            "hk-toggle-replay",
            "Toggle Replay Buffer",
            &s.hotkeys.toggle_replay,
        ))
        .child(hotkey_row(
            p,
            "hk-toggle-rec",
            "Start / Stop Recording",
            &s.hotkeys.toggle_record,
        ))
}

fn hotkey_row(p: &Palette, key: &str, label: &str, keycap: &str) -> El {
    row()
        .key(key)
        .w_full()
        .px(space::SM)
        .py(space::XS)
        .justify_between()
        .center_y()
        .fill(p.bg_lowest)
        .border(1.0, p.border_subtle)
        .radius(radius::SM)
        .child(
            text(label.to_string())
                .font(typography::BODY)
                .color(p.text_secondary),
        )
        .child(
            row()
                .key("keycap")
                .px(space::SM)
                .py(2.0)
                .fill(p.bg_surface)
                .border(1.0, p.border_active)
                .radius(radius::XS)
                .child(
                    text(keycap.to_string())
                        .font(typography::MONO_METRIC)
                        .color(p.accent_primary),
                ),
        )
}

fn fallback_notice(p: &Palette, key: &str, message: &str) -> El {
    row()
        .key(key)
        .w_full()
        .px(space::SM)
        .py(space::XS)
        .fill(p.warning_soft)
        .border(1.0, p.warning)
        .radius(radius::SM)
        .child(
            text(message.to_string())
                .font(typography::LABEL)
                .color(p.warning_text)
                .w_full()
                .wrap(),
        )
}

fn format_elapsed(started_at: DateTime<Utc>) -> String {
    let secs = Utc::now()
        .signed_duration_since(started_at)
        .num_seconds()
        .max(0);
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let rem = secs % 60;
    if hours > 0 {
        format!("{hours:02}:{mins:02}:{rem:02}")
    } else {
        format!("{mins:02}:{rem:02}")
    }
}

fn target_label(target: &CaptureTarget) -> String {
    match target {
        CaptureTarget::PrimaryScreen => "Primary Screen".to_string(),
        CaptureTarget::Monitor(name) => format!("Monitor ({name})"),
        CaptureTarget::FocusedWindow => "Focused Window".to_string(),
        CaptureTarget::Portal => "Wayland Portal".to_string(),
        CaptureTarget::Region {
            width,
            height,
            x,
            y,
        } => format!("Region ({width}×{height}+{x}+{y})"),
    }
}

fn video_codec_label(codec: VideoCodec) -> &'static str {
    match codec {
        VideoCodec::Auto => "Auto",
        VideoCodec::H264 => "H.264",
        VideoCodec::Hevc => "HEVC",
        VideoCodec::Av1 => "AV1",
        VideoCodec::Vp8 => "VP8",
        VideoCodec::Vp9 => "VP9",
    }
}

fn encoder_short_label(encoder: EncoderMode) -> &'static str {
    match encoder {
        EncoderMode::Gpu => "GPU",
        EncoderMode::Cpu => "CPU",
    }
}

#[cfg(test)]
mod tests {
    use clipforge_capture::AudioDeviceInfo;
    use rune::{Env, Runtime};

    use crate::state::{AppState, NavSection};
    use crate::views::root_view;

    fn long_pipewire_state() -> AppState {
        AppState {
            nav: NavSection::Recording,
            available_audio_devices: vec![
                AudioDeviceInfo {
                    id: "alsa_input.usb-Logitech_G_PRO_X_Gaming_Headset-00.mono-fallback"
                        .to_string(),
                    label: "Logitech G PRO X Gaming Headset".to_string(),
                },
                AudioDeviceInfo {
                    id: "alsa_output.usb-Logitech_G_PRO_X_Gaming_Headset-00.analog-stereo.monitor"
                        .to_string(),
                    label: "Monitor of Logitech G PRO X Gaming Headset Analog Stereo".to_string(),
                },
                AudioDeviceInfo {
                    id: "alsa_output.pci-0000_01_00.1.hdmi-stereo.monitor".to_string(),
                    label: "Monitor of GA104 High Definition Audio Controller Digital Stereo"
                        .to_string(),
                },
                AudioDeviceInfo {
                    id: "linuxsoundboard.virtual_mic_feeder".to_string(),
                    label: "linuxsoundboard.virtual_mic_feeder".to_string(),
                },
            ],
            available_app_audio: vec![AudioDeviceInfo {
                id: "app:Firefox:12345".to_string(),
                label: "Firefox Nightly Audio Stream (WebRTC Conference)".to_string(),
            }],
            ..AppState::default()
        }
    }

    fn assert_contained_horizontally(inner: [f32; 4], outer: [f32; 4], label: &str) {
        assert!(
            inner[0] >= outer[0] - 0.5 && inner[0] + inner[2] <= outer[0] + outer[2] + 0.5,
            "{label} horizontal overflow: inner={inner:?}, outer={outer:?}"
        );
    }

    fn assert_contained_vertically(inner: [f32; 4], outer: [f32; 4], label: &str) {
        assert!(
            inner[1] >= outer[1] - 0.5 && inner[1] + inner[3] <= outer[1] + outer[3] + 0.5,
            "{label} vertical overflow: inner={inner:?}, outer={outer:?}"
        );
    }

    fn rect_of(rt: &Runtime<AppState>, key: &str) -> [f32; 4] {
        rt.presented(key)
            .unwrap_or_else(|| panic!("missing presented rect for {key}"))
            .0
    }

    #[test]
    fn pipewire_device_list_renders_without_overlap_at_multiple_sizes() {
        for (width, height) in [(1280.0_f32, 800.0_f32), (960.0_f32, 640.0_f32)] {
            let mut rt = Runtime::new(
                long_pipewire_state(),
                root_view,
                (width, height),
                Env::new(true, 1.0),
            );
            rt.frame(0.0);

            let card_rect = rect_of(&rt, "rec-audio-card");
            let list_rect = rect_of(&rt, "rec-audio-list");
            assert_contained_horizontally(list_rect, card_rect, "rec-audio-list");
            assert!(
                list_rect[3] <= 261.0,
                "rec-audio-list should stay bounded for scrolling, got height {}",
                list_rect[3]
            );

            for key in [
                "audio-default-out",
                "audio-default-in",
                "audio-dev-0",
                "audio-dev-1",
                "audio-dev-2",
                "audio-dev-3",
                "audio-app-0",
            ] {
                let row_rect = rect_of(&rt, key);
                let chk_rect = rect_of(&rt, &format!("{key}-chk"));
                let info_rect = rect_of(&rt, &format!("{key}-info"));
                let label_rect = rect_of(&rt, &format!("{key}-label"));
                let id_rect = rect_of(&rt, &format!("{key}-id"));

                assert_contained_horizontally(row_rect, card_rect, key);
                assert_contained_horizontally(info_rect, row_rect, &format!("{key}-info"));
                assert_contained_horizontally(label_rect, row_rect, &format!("{key}-label"));
                assert_contained_horizontally(id_rect, row_rect, &format!("{key}-id"));
                assert_contained_vertically(label_rect, row_rect, &format!("{key}-label"));
                assert_contained_vertically(id_rect, row_rect, &format!("{key}-id"));

                assert!(
                    chk_rect[0] + chk_rect[2] <= info_rect[0] + 0.5,
                    "{key} checkbox overlaps text column: chk={chk_rect:?}, info={info_rect:?}"
                );
                assert!(
                    label_rect[1] + label_rect[3] <= id_rect[1] + 0.5,
                    "{key} label overlaps id vertically: label={label_rect:?}, id={id_rect:?}"
                );
            }
        }
    }

    #[test]
    fn pipewire_device_checkbox_toggles_source_and_updates_inspector() {
        let mut rt = Runtime::new(
            long_pipewire_state(),
            root_view,
            (1280.0, 1200.0),
            Env::new(true, 1.0),
        );
        rt.frame(0.0);

        let (cx, cy) = rt
            .find("audio-dev-1-chk")
            .expect("audio-dev-1-chk should be clickable");
        rt.pointer_down(cx, cy);
        rt.pointer_up(cx, cy);
        rt.frame(0.0);
        assert!(
            rt.state.config.capture.audio_sources.iter().any(|id| id
                == "alsa_output.usb-Logitech_G_PRO_X_Gaming_Headset-00.analog-stereo.monitor"),
            "clicking audio-dev-1-chk should select the PipeWire monitor source"
        );
        assert!(
            rt.presented("rec-audio-inspector").is_some(),
            "rec-audio-inspector should render when sources are selected"
        );
    }
}
