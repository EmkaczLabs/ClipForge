//! Video Editor view: preview area, timeline bar, trim-in/trim-out controls, and export.

use rune::tok::type_;
use rune::{El, col, el, row, text};

use crate::components::{
    badge, btn_primary, btn_secondary, card, kv_row, pill_option, section_header, toggle_switch,
};
use crate::state::{AppState, NavSection, format_duration_ms, is_sample_clip};
use crate::theme::{Palette, radius, snappy_spring, space};

pub fn view(p: &Palette, s: &AppState) -> El {
    let Some(clip) = s.selected_clip() else {
        return card(p, "editor-empty")
            .center()
            .py(space::XXL)
            .child(
                text("No clip selected")
                    .font(type_::SECTION)
                    .color(p.text_primary),
            )
            .child(btn_primary(p, "go-library", "Browse Clip Library", |st| {
                st.nav = NavSection::Library;
            }));
    };

    let clip_id = clip.id;
    let is_sample = is_sample_clip(clip);
    let source_label = if is_sample {
        "Sample clip (no file on disk)".to_string()
    } else {
        clip.file_path.display().to_string()
    };
    let duration_ms = clip.duration_ms.max(1_000);
    let start_ratio = (s.editor.trim_start_ms as f32 / duration_ms as f32).clamp(0.0, 1.0);
    let end_ratio = (s.editor.trim_end_ms as f32 / duration_ms as f32).clamp(start_ratio, 1.0);
    let selection_ms = s.editor.trim_end_ms.saturating_sub(s.editor.trim_start_ms);

    let track_w = 560.0;
    let sel_left = start_ratio * track_w;
    let sel_w = ((end_ratio - start_ratio) * track_w).max(8.0);

    let preview_card = card(p, "editor-preview-card")
        .child(
            row()
                .key("editor-clip-hdr")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    col()
                        .key("clip-meta")
                        .gap(2.0)
                        .child(
                            text(clip.title.clone())
                                .font(type_::SECTION)
                                .color(p.text_primary),
                        )
                        .child(
                            text(format!(
                                "{}x{} • {}fps • {}",
                                clip.width,
                                clip.height,
                                clip.fps as u32,
                                clip.codec.to_uppercase()
                            ))
                            .font(type_::LABEL)
                            .color(p.text_secondary),
                        ),
                )
                .child(
                    row()
                        .key("editor-hdr-badges")
                        .gap(space::SM)
                        .center_y()
                        .child(if is_sample {
                            badge("editor-sample-badge", "SAMPLE", p.warning, p.warning_soft)
                        } else {
                            badge("editor-rec-badge", "RECORDED", p.success, p.success_soft)
                        })
                        .child(badge(
                            "editor-dur-badge",
                            &format!("Duration {}", format_duration_ms(clip.duration_ms)),
                            p.text_primary,
                            p.bg_elevated,
                        )),
                ),
        )
        .child(
            col()
                .key("video-viewport")
                .w_full()
                .h(240.0)
                .center()
                .gap(space::SM)
                .radius(radius::MD)
                .fill(p.bg_base)
                .border(1.0, p.border_subtle)
                .child(
                    text(if s.editor.playing {
                        "Playing Preview"
                    } else {
                        "Video Preview Surface"
                    })
                    .font(type_::TITLE)
                    .color(p.text_primary),
                )
                .child(
                    text(format!(
                        "Playhead {} / {}",
                        format_duration_ms(s.editor.playhead_ms),
                        format_duration_ms(clip.duration_ms)
                    ))
                    .font(type_::LABEL)
                    .color(p.text_secondary),
                )
                .child(text(source_label).font(type_::LABEL).color(p.text_muted)),
        )
        .child(
            row()
                .key("playback-controls")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    row()
                        .key("transport")
                        .gap(space::SM)
                        .child(btn_secondary(p, "step-back", "-5s", |st| {
                            st.editor.playhead_ms = st.editor.playhead_ms.saturating_sub(5_000);
                        }))
                        .child(btn_primary(
                            p,
                            "play-pause",
                            if s.editor.playing { "Pause" } else { "Play" },
                            |st| st.editor.playing = !st.editor.playing,
                        ))
                        .child(btn_secondary(p, "step-fwd", "+5s", move |st| {
                            st.editor.playhead_ms =
                                (st.editor.playhead_ms + 5_000).min(duration_ms);
                        }))
                        .child(btn_secondary(
                            p,
                            "open-player",
                            "Open in Player",
                            move |st| st.open_clip_in_player(clip_id),
                        )),
                )
                .child(btn_secondary(
                    p,
                    "set-playhead-start",
                    "Jump to Trim In",
                    |st| {
                        st.editor.playhead_ms = st.editor.trim_start_ms;
                    },
                )),
        );

    let timeline_card = card(p, "editor-timeline-card")
        .child(section_header(
            p,
            "timeline-hdr",
            "Trim Timeline",
            "Adjust Trim-In and Trim-Out points before exporting",
        ))
        .child(
            el().key("timeline-track")
                .w(track_w)
                .h(28.0)
                .radius(radius::SM)
                .fill(p.bg_base)
                .border(1.0, p.border_subtle)
                .child(
                    el().key("timeline-selection")
                        .absolute(sel_left, 2.0)
                        .size(sel_w, 24.0)
                        .radius(radius::SM)
                        .fill(p.accent_soft)
                        .border(1.0, p.accent)
                        .motion(snappy_spring()),
                ),
        )
        .child(
            row()
                .key("trim-controls")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    row()
                        .key("trim-in-group")
                        .gap(space::SM)
                        .center_y()
                        .child(
                            text(format!(
                                "In: {}",
                                format_duration_ms(s.editor.trim_start_ms)
                            ))
                            .font(type_::BODY)
                            .color(p.text_primary),
                        )
                        .child(pill_option(p, "in-minus", "-1s", false, |st| {
                            st.editor.trim_start_ms = st.editor.trim_start_ms.saturating_sub(1_000);
                        }))
                        .child(pill_option(p, "in-plus", "+1s", false, |st| {
                            let next = st.editor.trim_start_ms + 1_000;
                            if next < st.editor.trim_end_ms {
                                st.editor.trim_start_ms = next;
                            }
                        })),
                )
                .child(badge(
                    "trim-len",
                    &format!("Selection: {}", format_duration_ms(selection_ms)),
                    p.accent_text,
                    p.accent_soft,
                ))
                .child(
                    row()
                        .key("trim-out-group")
                        .gap(space::SM)
                        .center_y()
                        .child(
                            text(format!("Out: {}", format_duration_ms(s.editor.trim_end_ms)))
                                .font(type_::BODY)
                                .color(p.text_primary),
                        )
                        .child(pill_option(p, "out-minus", "-1s", false, |st| {
                            let next = st.editor.trim_end_ms.saturating_sub(1_000);
                            if next > st.editor.trim_start_ms {
                                st.editor.trim_end_ms = next;
                            }
                        }))
                        .child(pill_option(p, "out-plus", "+1s", false, move |st| {
                            st.editor.trim_end_ms =
                                (st.editor.trim_end_ms + 1_000).min(duration_ms);
                        })),
                ),
        )
        .child(
            row()
                .key("export-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    row()
                        .key("reencode-toggle-row")
                        .gap(space::SM)
                        .center_y()
                        .child(toggle_switch(
                            p,
                            "reencode-switch",
                            "Re-encode for frame-accurate cut",
                            s.editor.reencode,
                            |st| st.editor.reencode = !st.editor.reencode,
                        ))
                        .child(
                            text("Frame-accurate re-encode")
                                .font(type_::LABEL)
                                .color(p.text_secondary),
                        ),
                )
                .child(
                    row()
                        .key("export-actions")
                        .gap(space::SM)
                        .child(btn_secondary(p, "trim-reset", "Reset Trim", move |st| {
                            st.editor.trim_start_ms = 0;
                            st.editor.trim_end_ms = duration_ms;
                        }))
                        .child(btn_primary(p, "trim-export", "Export Clip", |st| {
                            st.export_trimmed_clip();
                        })),
                ),
        );

    let mut root = col()
        .key("editor-view")
        .w_full()
        .gap(space::LG)
        .child(preview_card)
        .child(timeline_card);

    if let Some(export_path) = &s.editor.last_export_path {
        root = root.child(card(p, "export-result-card").child(kv_row(
            p,
            "last-export-row",
            "Last Export Output",
            export_path.display().to_string(),
        )));
    }

    root
}
