//! Home / Dashboard view: status summary, KPI stats, quick actions, and recent clips.

use clipforge_common::CaptureStatus;
use rune::tok::type_;
use rune::{El, col, row, text};

use crate::components::{
    badge, btn_danger, btn_primary, btn_secondary, card, section_header, stat_tile,
};
use crate::state::{AppState, NavSection, format_bytes, format_duration_ms, is_sample_clip};
use crate::theme::{Palette, space};

pub fn view(p: &Palette, s: &AppState) -> El {
    let total_ms: u64 = s.clips.iter().map(|c| c.duration_ms).sum();
    let total_bytes: u64 = s.clips.iter().map(|c| c.size_bytes).sum();
    let shared_count = s.uploads.len();

    let is_buffering = matches!(s.capture_status, CaptureStatus::ReplayBuffering { .. });
    let capture_error = match &s.capture_status {
        CaptureStatus::Error { message } => Some(message.as_str()),
        _ => None,
    };
    let gsr_badge = if capture_error.is_some() {
        badge("dash-gsr-err", "GSR Error", p.warning, p.warning_soft)
    } else if s.deps.gsr_available {
        badge("dash-gsr-ok", "GSR Ready", p.success, p.success_soft)
    } else {
        badge(
            "dash-gsr-missing",
            "GSR Not Installed",
            p.warning,
            p.warning_soft,
        )
    };

    let mut status_card = card(p, "dash-status-card")
        .child(
            row()
                .key("dash-status-top")
                .w_full()
                .justify_between()
                .center_y()
                .child(section_header(
                    p,
                    "dash-status-hdr",
                    "Capture Engine",
                    "Background replay buffer & instant clipping",
                ))
                .child(gsr_badge),
        )
        .child(
            row()
                .key("dash-status-actions")
                .gap(space::SM)
                .child(if is_buffering {
                    btn_danger(p, "dash-toggle-replay", "Stop Replay Buffer", |st| {
                        st.toggle_replay_buffer();
                    })
                } else {
                    btn_primary(
                        p,
                        "dash-toggle-replay",
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
                    "dash-save-replay",
                    "Save Last Replay",
                    |st| st.trigger_save_replay(),
                ))
                .child(btn_secondary(
                    p,
                    "dash-open-recording",
                    "Configure Capture",
                    |st| st.nav = NavSection::Recording,
                )),
        );
    if let Some(err_msg) = capture_error {
        status_card = status_card.child(
            row()
                .key("dash-capture-error")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    text(format!("Capture failed: {err_msg}"))
                        .font(type_::LABEL)
                        .color(p.warning),
                )
                .child(btn_secondary(p, "dash-clear-err", "Clear Error", |st| {
                    st.clear_capture_error();
                })),
        );
    }

    let stats_row = row()
        .key("dash-stats")
        .w_full()
        .gap(space::MD)
        .child(stat_tile(
            p,
            "stat-clips",
            "Total Clips",
            s.clips.len().to_string(),
            if s.using_sample_clips {
                "Sample data (no recordings yet)"
            } else {
                "Local library"
            },
        ))
        .child(stat_tile(
            p,
            "stat-duration",
            "Total Duration",
            format_duration_ms(total_ms),
            "Captured footage",
        ))
        .child(stat_tile(
            p,
            "stat-storage",
            "Disk Usage",
            format_bytes(total_bytes),
            "In clips folder",
        ))
        .child(stat_tile(
            p,
            "stat-shares",
            "Cloud Shares",
            shared_count.to_string(),
            "Uploaded links",
        ));

    let mut recent_list = col().key("dash-recent-list").gap(space::SM);
    for clip in s.clips.iter().take(4) {
        let id = clip.id;
        let game = clip
            .game
            .as_ref()
            .map(|g| g.name.as_str())
            .unwrap_or("Desktop");
        recent_list = recent_list.child(
            row()
                .key(format!("dash-clip-{id}"))
                .w_full()
                .px(space::MD)
                .py(space::SM)
                .justify_between()
                .center_y()
                .fill(p.bg_elevated)
                .border(1.0, p.border_subtle)
                .radius(crate::theme::radius::MD)
                .child(
                    col()
                        .key("meta")
                        .gap(2.0)
                        .child(
                            row()
                                .key("title-row")
                                .gap(space::SM)
                                .center_y()
                                .child(
                                    text(clip.title.clone())
                                        .font(type_::BODY)
                                        .color(p.text_primary),
                                )
                                .child(if is_sample_clip(clip) {
                                    badge("sample", "SAMPLE", p.warning, p.warning_soft)
                                } else {
                                    badge("recorded", "REC", p.success, p.success_soft)
                                }),
                        )
                        .child(
                            text(format!(
                                "{game} • {} • {}x{} @ {}fps",
                                format_duration_ms(clip.duration_ms),
                                clip.width,
                                clip.height,
                                clip.fps as u32
                            ))
                            .font(type_::LABEL)
                            .color(p.text_secondary),
                        ),
                )
                .child(
                    row()
                        .key("actions")
                        .gap(space::SM)
                        .child(btn_secondary(
                            p,
                            &format!("dash-edit-{id}"),
                            "Open Editor",
                            move |st| st.open_in_editor(id),
                        ))
                        .child(btn_secondary(
                            p,
                            &format!("dash-share-{id}"),
                            "Share",
                            move |st| st.queue_upload(id),
                        )),
                ),
        );
    }

    let recent_card = card(p, "dash-recent-card")
        .child(
            row()
                .key("dash-recent-hdr")
                .w_full()
                .justify_between()
                .center_y()
                .child(section_header(
                    p,
                    "dash-recent-title",
                    "Recent Clips",
                    "Jump back into trimming or sharing",
                ))
                .child(btn_secondary(p, "dash-view-all", "View All", |st| {
                    st.nav = NavSection::Library;
                })),
        )
        .child(recent_list);

    col()
        .key("dashboard-view")
        .w_full()
        .gap(space::LG)
        .child(status_card)
        .child(stats_row)
        .child(recent_card)
}
