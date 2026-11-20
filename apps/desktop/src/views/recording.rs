//! Recording view: GSR backend status, replay/record controls, capture config, and hotkeys.

use clipforge_common::{CaptureStatus, CaptureTarget, ContainerFormat, EncoderMode, QualityPreset};
use rune::tok::type_;
use rune::{El, col, row, text};

use crate::components::{
    badge, btn_danger, btn_primary, btn_secondary, card, kv_row, pill_option, section_header,
};
use crate::state::AppState;
use crate::theme::{Palette, space};

pub fn view(p: &Palette, s: &AppState) -> El {
    let is_buffering = matches!(s.capture_status, CaptureStatus::ReplayBuffering { .. });
    let is_recording = matches!(s.capture_status, CaptureStatus::Recording { .. });
    let capture_error = match &s.capture_status {
        CaptureStatus::Error { message } => Some(message.as_str()),
        _ => None,
    };

    let gsr_status = if capture_error.is_some() {
        badge("rec-gsr-error", "Capture Error", p.warning, p.warning_soft)
    } else if s.deps.gsr_available {
        badge(
            "rec-gsr-ready",
            s.deps
                .gsr_location
                .as_deref()
                .unwrap_or("gpu-screen-recorder ready"),
            p.success,
            p.success_soft,
        )
    } else {
        badge(
            "rec-gsr-missing",
            "gpu-screen-recorder not found on PATH/Flatpak",
            p.warning,
            p.warning_soft,
        )
    };

    let mut controls_card = card(p, "rec-controls-card")
        .child(
            row()
                .key("rec-hdr-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(section_header(
                    p,
                    "rec-hdr",
                    "GPU Screen Recorder",
                    "Zero-overhead replay buffer & live recording controls",
                ))
                .child(gsr_status),
        )
        .child(
            row()
                .key("rec-btn-row")
                .gap(space::SM)
                .child(if is_buffering {
                    btn_danger(p, "rec-replay-btn", "Stop Replay Buffer", |st| {
                        st.toggle_replay_buffer();
                    })
                } else {
                    btn_primary(
                        p,
                        "rec-replay-btn",
                        if capture_error.is_some() {
                            "Restart Replay Buffer"
                        } else {
                            "Start Replay Buffer"
                        },
                        |st| {
                            st.toggle_replay_buffer();
                        },
                    )
                })
                .child(btn_secondary(
                    p,
                    "rec-save-replay-btn",
                    "Save Replay",
                    |st| st.trigger_save_replay(),
                ))
                .child(if is_recording {
                    btn_danger(p, "rec-record-btn", "Stop Recording", |st| {
                        st.toggle_recording();
                    })
                } else {
                    btn_secondary(p, "rec-record-btn", "Start Recording", |st| {
                        st.toggle_recording();
                    })
                }),
        );
    if let Some(err_msg) = capture_error {
        controls_card = controls_card.child(
            row()
                .key("rec-capture-error")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    text(format!("Capture failed: {err_msg}"))
                        .font(type_::LABEL)
                        .color(p.warning),
                )
                .child(btn_secondary(p, "rec-clear-err", "Dismiss Error", |st| {
                    st.clear_capture_error();
                })),
        );
    }

    let cfg = &s.config.capture;

    let target_row = row()
        .key("cfg-target-row")
        .w_full()
        .justify_between()
        .center_y()
        .child(
            text("Capture Target")
                .font(type_::BODY)
                .color(p.text_secondary),
        )
        .child(
            row()
                .key("target-opts")
                .gap(space::SM)
                .child(pill_option(
                    p,
                    "target-screen",
                    "Primary Screen",
                    cfg.capture_target == CaptureTarget::PrimaryScreen,
                    |st| st.set_capture_target(CaptureTarget::PrimaryScreen),
                ))
                .child(pill_option(
                    p,
                    "target-focused",
                    "Focused Window",
                    cfg.capture_target == CaptureTarget::FocusedWindow,
                    |st| st.set_capture_target(CaptureTarget::FocusedWindow),
                ))
                .child(pill_option(
                    p,
                    "target-portal",
                    "Wayland Portal",
                    cfg.capture_target == CaptureTarget::Portal,
                    |st| st.set_capture_target(CaptureTarget::Portal),
                )),
        );

    let fps_row = row()
        .key("cfg-fps-row")
        .w_full()
        .justify_between()
        .center_y()
        .child(text("Frame Rate").font(type_::BODY).color(p.text_secondary))
        .child(
            row()
                .key("fps-opts")
                .gap(space::SM)
                .child(pill_option(p, "fps-30", "30 FPS", cfg.fps == 30, |st| {
                    st.config.capture.fps = 30;
                }))
                .child(pill_option(p, "fps-60", "60 FPS", cfg.fps == 60, |st| {
                    st.config.capture.fps = 60;
                }))
                .child(pill_option(p, "fps-120", "120 FPS", cfg.fps == 120, |st| {
                    st.config.capture.fps = 120
                })),
        );

    let buffer_row = row()
        .key("cfg-buf-row")
        .w_full()
        .justify_between()
        .center_y()
        .child(
            text("Replay Buffer Length")
                .font(type_::BODY)
                .color(p.text_secondary),
        )
        .child(
            row()
                .key("buf-opts")
                .gap(space::SM)
                .child(pill_option(
                    p,
                    "buf-30",
                    "30s",
                    cfg.replay_buffer_secs == 30,
                    |st| st.config.capture.replay_buffer_secs = 30,
                ))
                .child(pill_option(
                    p,
                    "buf-60",
                    "60s",
                    cfg.replay_buffer_secs == 60,
                    |st| st.config.capture.replay_buffer_secs = 60,
                ))
                .child(pill_option(
                    p,
                    "buf-120",
                    "120s",
                    cfg.replay_buffer_secs == 120,
                    |st| st.config.capture.replay_buffer_secs = 120,
                ))
                .child(pill_option(
                    p,
                    "buf-300",
                    "5m",
                    cfg.replay_buffer_secs == 300,
                    |st| st.config.capture.replay_buffer_secs = 300,
                )),
        );

    let quality_row = row()
        .key("cfg-qual-row")
        .w_full()
        .justify_between()
        .center_y()
        .child(text("Quality").font(type_::BODY).color(p.text_secondary))
        .child(
            row()
                .key("qual-opts")
                .gap(space::SM)
                .child(pill_option(
                    p,
                    "q-med",
                    "Medium",
                    cfg.quality == QualityPreset::Medium,
                    |st| st.set_quality(QualityPreset::Medium),
                ))
                .child(pill_option(
                    p,
                    "q-high",
                    "High",
                    cfg.quality == QualityPreset::High,
                    |st| st.set_quality(QualityPreset::High),
                ))
                .child(pill_option(
                    p,
                    "q-vhigh",
                    "Very High",
                    cfg.quality == QualityPreset::VeryHigh,
                    |st| st.set_quality(QualityPreset::VeryHigh),
                ))
                .child(pill_option(
                    p,
                    "q-ultra",
                    "Ultra",
                    cfg.quality == QualityPreset::Ultra,
                    |st| st.set_quality(QualityPreset::Ultra),
                )),
        );

    let encoder_row = row()
        .key("cfg-enc-row")
        .w_full()
        .justify_between()
        .center_y()
        .child(
            text("Encoder & Container")
                .font(type_::BODY)
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
                    cfg.encoder == EncoderMode::Gpu,
                    |st| st.set_encoder(EncoderMode::Gpu),
                ))
                .child(pill_option(
                    p,
                    "enc-cpu",
                    "CPU",
                    cfg.encoder == EncoderMode::Cpu,
                    |st| st.set_encoder(EncoderMode::Cpu),
                ))
                .child(pill_option(
                    p,
                    "cont-mp4",
                    "MP4",
                    cfg.container == ContainerFormat::Mp4,
                    |st| st.set_container(ContainerFormat::Mp4),
                ))
                .child(pill_option(
                    p,
                    "cont-mkv",
                    "MKV",
                    cfg.container == ContainerFormat::Mkv,
                    |st| st.set_container(ContainerFormat::Mkv),
                )),
        );

    let config_card = card(p, "rec-config-card")
        .child(section_header(
            p,
            "rec-cfg-hdr",
            "Capture Profile",
            "Applied when starting gpu-screen-recorder",
        ))
        .child(target_row)
        .child(fps_row)
        .child(buffer_row)
        .child(quality_row)
        .child(encoder_row);

    let hotkey_card = card(p, "rec-hotkey-card")
        .child(section_header(
            p,
            "rec-hk-hdr",
            "Global Hotkeys",
            "X11 keybindings / Wayland GlobalShortcuts portal",
        ))
        .child(kv_row(
            p,
            "hk-save",
            "Save Replay Buffer",
            s.hotkeys.save_replay.clone(),
        ))
        .child(kv_row(
            p,
            "hk-toggle-replay",
            "Toggle Replay Buffer",
            s.hotkeys.toggle_replay.clone(),
        ))
        .child(kv_row(
            p,
            "hk-toggle-rec",
            "Start / Stop Recording",
            s.hotkeys.toggle_record.clone(),
        ));

    col()
        .key("recording-view")
        .w_full()
        .gap(space::LG)
        .child(controls_card)
        .child(config_card)
        .child(hotkey_card)
}
