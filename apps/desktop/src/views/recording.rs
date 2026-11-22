//! Recording Studio view: live GSR status, replay/record controls, capture pipeline, audio, and hotkeys.

use chrono::{DateTime, Utc};
use clipforge_capture::{AudioTrackTarget, GlobalHotkeyStatus, parse_routed_audio_source};
use clipforge_common::{
    AudioCodec, CaptureStatus, CaptureTarget, ContainerFormat, EncoderMode, QualityPreset,
    VideoCodec,
};
use clipforge_media::{DBFS_FLOOR, MicFeedbackKind, MicTestPhase};
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
                    .child(mic_test_card(p, s))
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
    } else if cfg!(target_os = "windows") {
        ("Capture Missing", p.warning_text, p.warning_soft)
    } else {
        ("GSR Missing", p.warning_text, p.warning_soft)
    };

    let engine_row_label = if cfg!(target_os = "windows") {
        "Capture Engine"
    } else {
        "GSR Engine"
    };

    let gsr_val = s.deps.gsr_location.clone().unwrap_or_else(|| {
        if s.deps.gsr_available {
            if cfg!(target_os = "windows") {
                "Windows Graphics Capture + WASAPI".to_string()
            } else {
                "Available".to_string()
            }
        } else if cfg!(target_os = "windows") {
            "FFmpeg missing from PATH".to_string()
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
        .child(kv_row(p, "rec-hlth-gsr", engine_row_label, gsr_val))
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
                text(if cfg!(target_os = "windows") {
                    "Click Refresh Sources to enumerate displays"
                } else {
                    "Click Refresh Sources to enumerate displays from GSR"
                })
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

    if s.audio_config_dirty_while_active && !matches!(s.capture_status, CaptureStatus::Idle) {
        c = c.child(audio_restart_banner(p, s));
    }

    c = c
        .child(audio_source_row(
            p,
            "audio-default-out",
            "default_output",
            "System Desktop Audio",
            s.audio_source_track("default_output"),
        ))
        .child(audio_source_row(
            p,
            "audio-default-in",
            "default_input",
            "Default Microphone",
            s.audio_source_track("default_input"),
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
                let track = s.audio_source_track(&dev.id);
                list = list.child(audio_source_row(
                    p,
                    &format!("audio-dev-{idx}"),
                    &dev.id,
                    &dev.label,
                    track,
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
                let track = s.audio_source_track(&app.id);
                list = list.child(audio_source_row(
                    p,
                    &format!("audio-app-{idx}"),
                    &app.id,
                    &app.label,
                    track,
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

    c = c.child(track_diagnostics_panel(p, s));

    c
}

fn audio_restart_banner(p: &Palette, s: &AppState) -> El {
    let mut box_col = col()
        .key("rec-audio-restart-banner")
        .w_full()
        .min_w(0.0)
        .px(space::SM)
        .py(space::SM)
        .gap(space::SM)
        .fill(p.warning_soft)
        .border(1.0, p.warning)
        .radius(radius::SM)
        .child(
            text("Audio config changed while active. Restart replay buffer to apply.")
                .font(typography::LABEL)
                .color(p.warning_text)
                .w_full()
                .wrap(),
        );
    if matches!(s.capture_status, CaptureStatus::ReplayBuffering { .. }) {
        box_col = box_col.child(btn_primary(
            p,
            "rec-audio-restart-btn",
            "Restart Replay Buffer",
            |st| st.restart_replay_buffer(),
        ));
    }
    box_col
}

fn track_diagnostics_panel(p: &Palette, s: &AppState) -> El {
    let cfg = &s.config.capture;
    let diag = s.audio_diagnostics();

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
            text("TRACK DIAGNOSTICS")
                .font(typography::MONO_BADGE)
                .color(p.text_muted),
        );

    if cfg.audio_sources.is_empty() {
        inspector = inspector.child(
            text("No audio sources selected. Enable at least one source above.")
                .font(typography::LABEL)
                .color(p.text_muted)
                .w_full()
                .wrap(),
        );
    } else {
        for (idx, raw_src) in cfg.audio_sources.iter().enumerate() {
            let (target, clean_id) = parse_routed_audio_source(raw_src)
                .unwrap_or((AudioTrackTarget::Auto, raw_src.clone()));
            let full_label = resolve_audio_source_label(s, &clean_id);
            inspector = inspector.child(
                col()
                    .key(format!("rec-audio-active-{idx}"))
                    .w_full()
                    .min_w(0.0)
                    .gap(2.0)
                    .child(
                        row()
                            .key(format!("rec-audio-active-hdr-{idx}"))
                            .w_full()
                            .min_w(0.0)
                            .gap(space::XS)
                            .justify_between()
                            .center_y()
                            .child(
                                text(full_label)
                                    .grow(1.0)
                                    .min_w(0.0)
                                    .font(typography::LABEL)
                                    .color(p.text_primary)
                                    .wrap(),
                            )
                            .child(badge(
                                &format!("rec-audio-active-trk-{idx}"),
                                target.label(),
                                p.accent_primary,
                                p.accent_soft,
                            )),
                    )
                    .child(
                        text(clean_id)
                            .w_full()
                            .font(typography::MONO_BADGE)
                            .color(p.text_muted)
                            .wrap_anywhere(),
                    ),
            );
        }
    }

    let gsr_args_text = if diag.routing_plan.gsr_audio_args.is_empty() {
        "None".to_string()
    } else {
        diag.routing_plan
            .gsr_audio_args
            .iter()
            .map(|a| format!("-a \"{a}\""))
            .collect::<Vec<_>>()
            .join(" ")
    };

    inspector = inspector.child(kv_row(p, "diag-gsr-args", "GSR Audio Args", gsr_args_text));

    for (idx, trk) in diag.routing_plan.tracks.iter().enumerate() {
        inspector = inspector.child(kv_row(
            p,
            &format!("diag-track-{idx}"),
            &format!("{} (0:a:{})", trk.track_label, trk.stream_index),
            trk.gsr_arg.clone(),
        ));
    }

    let stream_summary = match &diag.last_saved_clip_title {
        Some(title) => format!("{} ({})", diag.last_saved_stream_count, title),
        None => format!(
            "{} planned (no clip saved yet)",
            diag.routing_plan.tracks.len()
        ),
    };
    inspector = inspector.child(kv_row(p, "diag-streams", "Encoded Streams", stream_summary));

    let codec_channels = match (&diag.last_saved_codec, diag.last_saved_channels) {
        (Some(codec), Some(ch)) => format!("{codec} • {ch} ch"),
        (Some(codec), None) => codec.clone(),
        _ => format!("{} (configured)", audio_codec_label(cfg.audio_codec)),
    };
    inspector = inspector.child(kv_row(
        p,
        "diag-codec-ch",
        "Codec & Channels",
        codec_channels,
    ));

    let mic_signal = match diag.mic_signal_detected {
        Some(true) => "Detected",
        Some(false) => "Not Detected",
        None => "Not Tested",
    };
    inspector = inspector.child(kv_row(
        p,
        "diag-mic-signal",
        "Mic Signal",
        mic_signal.to_string(),
    ));

    inspector
}

fn audio_codec_label(codec: AudioCodec) -> &'static str {
    match codec {
        AudioCodec::Opus => "Opus",
        AudioCodec::Aac => "AAC",
        AudioCodec::Flac => "FLAC",
    }
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

fn audio_source_row(
    p: &Palette,
    key: &str,
    id: &str,
    label: &str,
    track: Option<AudioTrackTarget>,
) -> El {
    let checked = track.is_some();
    let source_id_toggle = id.to_string();
    let mut container = col()
        .key(key)
        .w_full()
        .min_w(0.0)
        .px(space::SM)
        .py(space::SM)
        .gap(space::XS)
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
            row()
                .key(format!("{key}-top"))
                .w_full()
                .min_w(0.0)
                .gap(space::SM)
                .center_y()
                .child(
                    checkbox(p, &format!("{key}-chk"), label, checked, move |st| {
                        st.toggle_audio_source(source_id_toggle.clone());
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
                ),
        );

    if let Some(active_track) = track {
        let mut track_row = row()
            .key(format!("{key}-tracks"))
            .w_full()
            .min_w(0.0)
            .gap(space::XS)
            .center_y()
            .child(
                text("Track:")
                    .font(typography::MONO_BADGE)
                    .color(p.text_muted),
            );
        for target in [
            AudioTrackTarget::Auto,
            AudioTrackTarget::T1,
            AudioTrackTarget::T2,
            AudioTrackTarget::T3,
            AudioTrackTarget::T4,
        ] {
            let src = id.to_string();
            track_row = track_row.child(pill_option(
                p,
                &format!("{key}-trk-{}", target.label()),
                target.label(),
                active_track == target,
                move |st| st.set_audio_source_track(&src, target),
            ));
        }
        container = container.child(track_row);
    }

    container
}

fn mic_test_card(p: &Palette, s: &AppState) -> El {
    let mic = &s.mic_test;
    let selected_id = mic
        .selected_source
        .clone()
        .or_else(|| s.selected_mic_source());
    let selected_label = selected_id
        .as_deref()
        .map(|id| resolve_audio_source_label(s, id))
        .unwrap_or_else(|| "No microphone selected".to_string());

    let meter_progress = if mic.rms_dbfs <= DBFS_FLOOR + 0.5 {
        0.0
    } else {
        ((mic.rms_dbfs + 60.0) / 60.0).clamp(0.0, 1.0)
    };
    let meter_color = if mic.peak_dbfs >= -3.0 {
        p.recording
    } else if mic.rms_dbfs >= -45.0 {
        p.success
    } else {
        p.warning
    };

    let dbfs_readout = if mic.rms_dbfs <= DBFS_FLOOR + 0.5 && mic.peak_dbfs <= DBFS_FLOOR + 0.5 {
        "-- dBFS (Idle)".to_string()
    } else {
        format!(
            "RMS {:.1} dBFS  •  Peak {:.1} dBFS",
            mic.rms_dbfs, mic.peak_dbfs
        )
    };

    let mut actions = row()
        .key("rec-mic-test-actions")
        .w_full()
        .gap(space::SM)
        .center_y();

    match mic.phase {
        MicTestPhase::Recording => {
            actions = actions
                .child(btn_danger(p, "rec-mic-stop-btn", "Stop Test", |st| {
                    st.stop_mic_test();
                }))
                .child(
                    text(format!("{:.1}s / 3.0s", mic.elapsed_ms as f32 / 1000.0))
                        .font(typography::MONO_BADGE)
                        .color(p.text_secondary),
                );
        }
        MicTestPhase::Playing => {
            actions = actions.child(btn_danger(
                p,
                "rec-mic-stop-play-btn",
                "Stop Playback",
                |st| st.stop_mic_test(),
            ));
        }
        _ => {
            actions = actions.child(btn_primary(
                p,
                "rec-mic-start-btn",
                "Test Microphone",
                |st| st.start_mic_test(),
            ));
            if mic.diagnostic_summary.is_some() {
                actions =
                    actions.child(btn_secondary(p, "rec-mic-play-btn", "Play Sample", |st| {
                        st.play_mic_test_sample()
                    }));
            }
        }
    }

    let mut c = card(p, "rec-mic-test-card")
        .child(section_header(
            p,
            "rec-mic-test-hdr",
            "Microphone Test",
            "Live PipeWire sample level & playback verification",
        ))
        .child(
            col()
                .key("rec-mic-selected")
                .w_full()
                .min_w(0.0)
                .gap(2.0)
                .child(
                    text(selected_label)
                        .font(typography::LABEL)
                        .color(p.text_primary)
                        .w_full()
                        .wrap(),
                )
                .child(
                    text(
                        selected_id
                            .unwrap_or_else(|| "Select a microphone in Audio Sources".to_string()),
                    )
                    .font(typography::MONO_BADGE)
                    .color(p.text_muted)
                    .w_full()
                    .wrap_anywhere(),
                ),
        )
        .child(
            col()
                .key("rec-mic-meter-box")
                .w_full()
                .gap(space::XS)
                .child(progress_bar(
                    p,
                    "rec-mic-meter",
                    meter_progress,
                    meter_color,
                ))
                .child(
                    text(dbfs_readout)
                        .font(typography::MONO_BADGE)
                        .color(p.text_secondary),
                ),
        )
        .child(actions);

    if let Some(feedback) = mic.feedback {
        let (label, fg, bg) = mic_feedback_badge(p, feedback);
        c = c.child(
            row()
                .key("rec-mic-feedback-row")
                .w_full()
                .gap(space::SM)
                .center_y()
                .child(badge("rec-mic-feedback-badge", label, fg, bg)),
        );
    }

    if let Some(err) = &mic.error {
        c = c.child(
            text(err.clone())
                .key("rec-mic-error-text")
                .font(typography::LABEL)
                .color(p.error_text)
                .w_full()
                .wrap(),
        );
    }

    if let Some(summary) = &mic.diagnostic_summary {
        c = c.child(
            col()
                .key("rec-mic-summary")
                .w_full()
                .min_w(0.0)
                .gap(space::XS)
                .px(space::SM)
                .py(space::XS)
                .fill(p.bg_lowest)
                .border(1.0, p.border_subtle)
                .radius(radius::SM)
                .child(kv_row(
                    p,
                    "mic-sum-device",
                    "Backend Device",
                    summary
                        .resolved_backend_device
                        .clone()
                        .unwrap_or_else(|| "Unavailable".to_string()),
                ))
                .child(kv_row(
                    p,
                    "mic-sum-fmt",
                    "Format",
                    format!(
                        "{} • {} Hz • {} ch",
                        summary.backend_kind, summary.sample_rate, summary.channels
                    ),
                ))
                .child(kv_row(
                    p,
                    "mic-sum-signal",
                    "Signal Detected",
                    if summary.signal_detected { "Yes" } else { "No" }.to_string(),
                ))
                .child(
                    text(summary.message.clone())
                        .font(typography::LABEL)
                        .color(p.text_secondary)
                        .w_full()
                        .wrap(),
                ),
        );
    }

    c
}

fn mic_feedback_badge(
    p: &Palette,
    kind: MicFeedbackKind,
) -> (&'static str, rune::Color, rune::Color) {
    match kind {
        MicFeedbackKind::Silence => ("Silence", p.warning_text, p.warning_soft),
        MicFeedbackKind::MutedOrVeryQuiet => ("Muted / Very Quiet", p.warning_text, p.warning_soft),
        MicFeedbackKind::GoodSignal => ("Good Signal", p.success_text, p.success_soft),
        MicFeedbackKind::Clipping => ("Clipping", p.error_text, p.recording_soft),
        MicFeedbackKind::MissingDevice => ("Missing Device", p.error_text, p.recording_soft),
        MicFeedbackKind::PermissionError => ("Permission Error", p.error_text, p.recording_soft),
        MicFeedbackKind::Success => ("Playback OK", p.success_text, p.success_soft),
    }
}

fn hotkeys_card(p: &Palette, s: &AppState) -> El {
    let (badge_label, badge_fg, badge_bg, subtitle, detail_notice) = match &s.hotkey_status {
        GlobalHotkeyStatus::WaylandPortalActive => (
            "Portal Active".to_string(),
            p.success_text,
            p.success_soft,
            "GlobalShortcuts (Wayland Portal Active)".to_string(),
            None,
        ),
        GlobalHotkeyStatus::X11GlobalActive => (
            "X11 Global".to_string(),
            p.success_text,
            p.success_soft,
            "X11 global keybindings active".to_string(),
            None,
        ),
        GlobalHotkeyStatus::WindowsGlobalActive => (
            "Windows Global".to_string(),
            p.success_text,
            p.success_soft,
            "Windows global hotkeys (RegisterHotKey) active".to_string(),
            None,
        ),
        GlobalHotkeyStatus::PermissionPending => (
            "Pending".to_string(),
            p.warning_text,
            p.warning_soft,
            "Permission Pending — approve in system dialog".to_string(),
            Some(
                "Approve the desktop portal GlobalShortcuts prompt to enable background hotkeys."
                    .to_string(),
            ),
        ),
        GlobalHotkeyStatus::FocusedWindowOnly { reason } => (
            "Focused Only".to_string(),
            p.text_secondary,
            p.bg_elevated,
            "Focused-Window Only (F9/F10/F11 work when focused)".to_string(),
            reason
                .as_ref()
                .map(|r| format!("Global registration inactive: {r}")),
        ),
        GlobalHotkeyStatus::RegistrationError { message } => (
            "Error".to_string(),
            p.warning_text,
            p.warning_soft,
            "Registration Error (focused shortcuts active)".to_string(),
            Some(format!("Shortcut registration error: {message}")),
        ),
    };

    let mut out = card(p, "rec-hotkey-card")
        .child(
            row()
                .key("rec-hk-hdr-row")
                .w_full()
                .justify_between()
                .center_y()
                .gap(space::SM)
                .child(section_header(p, "rec-hk-hdr", "Global Hotkeys", &subtitle))
                .child(badge("rec-hk-status", &badge_label, badge_fg, badge_bg)),
        )
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
        ));

    if let Some(notice) = detail_notice {
        out = out.child(fallback_notice(p, "rec-hk-notice", &notice));
    }
    out
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

    #[test]
    fn track_routing_pills_and_restart_banner_work() {
        use chrono::Utc;
        use clipforge_capture::AudioTrackTarget;
        use clipforge_common::CaptureStatus;

        let mut st = long_pipewire_state();
        st.config.capture.audio_sources =
            vec!["default_output".to_string(), "default_input".to_string()];
        st.capture_status = CaptureStatus::ReplayBuffering {
            started_at: Utc::now(),
            buffer_secs: 60,
        };
        let mut rt = Runtime::new(st, root_view, (1280.0, 1200.0), Env::new(true, 1.0));
        rt.frame(0.0);

        // Assign default_input to T2 via pill click
        let (tx, ty) = rt
            .find("audio-default-in-trk-T2")
            .expect("audio-default-in-trk-T2 pill should be clickable");
        rt.pointer_down(tx, ty);
        rt.pointer_up(tx, ty);
        rt.frame(0.5);

        assert_eq!(
            rt.state.audio_source_track("default_input"),
            Some(AudioTrackTarget::T2)
        );
        assert!(rt.state.audio_config_dirty_while_active);
        assert!(rt.presented("rec-audio-restart-banner").is_some());
        assert!(rt.presented("diag-gsr-args").is_some());

        let (rx, ry) = rt
            .find("rec-audio-restart-btn")
            .expect("rec-audio-restart-btn should be clickable");
        rt.pointer_down(rx, ry);
        rt.pointer_up(rx, ry);
        rt.frame(0.5);
        assert!(!rt.state.audio_config_dirty_while_active);
    }

    #[test]
    fn mic_test_card_renders_level_meter_feedback_and_summary() {
        use clipforge_media::{MicDiagnosticSummary, MicFeedbackKind, MicTestPhase};

        let mut st = long_pipewire_state();
        st.mic_test.phase = MicTestPhase::Recorded;
        st.mic_test.selected_source = Some("default_input".to_string());
        st.mic_test.rms_dbfs = -18.5;
        st.mic_test.peak_dbfs = -6.2;
        st.mic_test.feedback = Some(MicFeedbackKind::GoodSignal);
        st.mic_test.diagnostic_summary = Some(MicDiagnosticSummary {
            selected_source_id: "default_input".to_string(),
            resolved_backend_device: Some("easyeffects_source".to_string()),
            backend_kind: "pipewire-pulse".to_string(),
            sample_rate: 48_000,
            channels: 1,
            rms_dbfs: -18.5,
            peak_dbfs: -6.2,
            signal_detected: true,
            source_muted: Some(false),
            recorded_duration_ms: 3000,
            feedback: MicFeedbackKind::GoodSignal,
            message: "Good microphone signal detected.".to_string(),
        });

        let mut rt = Runtime::new(st, root_view, (1280.0, 1400.0), Env::new(true, 1.0));
        rt.frame(0.0);

        assert!(rt.presented("rec-mic-test-card").is_some());
        assert!(rt.presented("rec-mic-meter").is_some());
        assert!(rt.presented("rec-mic-feedback-badge").is_some());
        assert!(rt.presented("rec-mic-summary").is_some());
        assert!(rt.find("rec-mic-start-btn").is_some());
        assert!(rt.find("rec-mic-play-btn").is_some());

        let (sx, sy) = rt
            .find("rec-mic-start-btn")
            .expect("rec-mic-start-btn should be clickable");
        rt.pointer_down(sx, sy);
        rt.pointer_up(sx, sy);
        rt.frame(0.0);
        assert_eq!(rt.state.mic_test.phase, MicTestPhase::Recording);
        assert!(rt.find("rec-mic-stop-btn").is_some());
    }
}
