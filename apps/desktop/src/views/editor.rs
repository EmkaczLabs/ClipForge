//! Video Editor view: embedded video viewport, interactive timeline, trim handles, and export.

use rune::tok::type_;
use rune::{Cursor, Drag, El, Phase, SurfaceFit, Tx, col, el, row, text};

use crate::components::{
    badge, btn_primary, btn_secondary, card, kv_row, pill_option, section_header, toggle_switch,
};
use crate::state::{AppState, NavSection, format_duration_ms, is_sample_clip};
use crate::theme::{Palette, radius, space};

/// Formats milliseconds as `MM:SS.cc` for accurate timeline/trim readouts.
fn format_timecode_ms(ms: u64) -> String {
    let total_secs = ms / 1_000;
    let minutes = total_secs / 60;
    let seconds = total_secs % 60;
    let centis = (ms % 1_000) / 10;
    format!("{minutes:02}:{seconds:02}.{centis:02}")
}

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
    let playhead_ratio = (s.editor.playhead_ms as f32 / duration_ms as f32).clamp(0.0, 1.0);
    let selection_ms = s.editor.trim_end_ms.saturating_sub(s.editor.trim_start_ms);

    let track_w: f32 = 640.0;
    let track_h: f32 = 56.0;
    let handle_w: f32 = 10.0;
    let sel_left = start_ratio * track_w;
    let sel_w = ((end_ratio - start_ratio) * track_w).max(handle_w * 2.0);
    let playhead_x = (playhead_ratio * track_w).clamp(0.0, track_w - 3.0);

    let viewport_content = if is_sample {
        col()
            .key("video-sample-placeholder")
            .center()
            .gap(space::SM)
            .child(
                text("Sample Clip Preview")
                    .font(type_::TITLE)
                    .color(p.text_primary),
            )
            .child(
                text("Sample clips have no video file on disk. Record or scan a real clip to play video.")
                    .font(type_::LABEL)
                    .color(p.text_secondary),
            )
            .child(text(source_label).font(type_::LABEL).color(p.text_muted))
    } else if let Some(err) = &s.editor.decoder_error {
        col()
            .key("video-decoder-error")
            .center()
            .gap(space::SM)
            .child(
                text("Video Decoder Error")
                    .font(type_::TITLE)
                    .color(p.recording),
            )
            .child(text(err.clone()).font(type_::LABEL).color(p.text_secondary))
            .child(btn_secondary(p, "retry-decode", "Retry Playback", |st| {
                st.restart_editor();
            }))
    } else if s.editor.has_video_frame {
        el().key("video-surface")
            .w_full()
            .h_full()
            .cursor(Cursor::Pointer)
            .pixels_fit(&s.editor.video_pixels, SurfaceFit::Fit)
            .on_press(|st: &mut AppState| st.toggle_play())
    } else {
        col()
            .key("video-loading")
            .center()
            .gap(space::SM)
            .child(
                text("Decoding video frame...")
                    .font(type_::SECTION)
                    .color(p.text_primary),
            )
            .child(text(source_label).font(type_::LABEL).color(p.text_muted))
    };

    let res_label = if s.editor.video_width > 0 && s.editor.video_height > 0 {
        format!("{}x{}", s.editor.video_width, s.editor.video_height)
    } else {
        format!("{}x{}", clip.width, clip.height)
    };

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
                                "{} • {}fps • {}",
                                res_label,
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
                .h(320.0)
                .center()
                .radius(radius::MD)
                .fill(p.bg_base)
                .border(1.0, p.border_subtle)
                .clip_rounded(radius::MD)
                .child(viewport_content),
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
                        .center_y()
                        .child(btn_secondary(p, "step-back", "-5s", |st| {
                            let target = st.editor.playhead_ms.saturating_sub(5_000);
                            st.seek_editor(target);
                        }))
                        .child(btn_primary(
                            p,
                            "play-pause",
                            if s.editor.playing { "Pause" } else { "Play" },
                            |st| st.toggle_play(),
                        ))
                        .child(btn_secondary(p, "restart-trim", "Restart", |st| {
                            st.restart_editor();
                        }))
                        .child(btn_secondary(p, "step-fwd", "+5s", move |st| {
                            let target = (st.editor.playhead_ms + 5_000).min(duration_ms);
                            st.seek_editor(target);
                        }))
                        .child(btn_secondary(
                            p,
                            "open-player",
                            "Open in Player",
                            move |st| st.open_clip_in_player(clip_id),
                        )),
                )
                .child(
                    row()
                        .key("timecode-group")
                        .gap(space::SM)
                        .center_y()
                        .child(
                            text(format!(
                                "{} / {}",
                                format_timecode_ms(s.editor.playhead_ms),
                                format_timecode_ms(duration_ms)
                            ))
                            .font(type_::LABEL)
                            .color(p.text_primary),
                        )
                        .child(btn_secondary(
                            p,
                            "set-playhead-start",
                            "Jump to Trim In",
                            |st| {
                                let start = st.editor.trim_start_ms;
                                st.seek_editor(start);
                            },
                        )),
                ),
        );

    let mut thumb_strip = row().key("timeline-thumbs").w_full().h_full();
    for (idx, thumb) in s.editor.timeline_thumbs.iter().enumerate() {
        thumb_strip = thumb_strip.child(
            el().key(format!("timeline-thumb-{idx}"))
                .grow(1.0)
                .h_full()
                .pixels_fit(thumb, SurfaceFit::Fill),
        );
    }

    let trim_in_handle = el()
        .key("trim-in-handle")
        .absolute(sel_left.clamp(0.0, track_w - handle_w), 0.0)
        .size(handle_w, track_h)
        .radius(radius::SM)
        .fill(p.accent)
        .cursor(Cursor::ResizeColumn)
        .on_drag(
            move |st: &mut AppState, d: &mut Drag, _tx: &mut Tx| match d.phase {
                Phase::Down => {
                    d.data = st.editor.trim_start_ms as f32;
                }
                Phase::Move | Phase::Up => {
                    if d.cancelled() {
                        st.set_trim_start(d.data.round().max(0.0) as u64);
                        return;
                    }
                    let delta_ms = (d.dx / track_w) * duration_ms as f32;
                    let next_ms = (d.data + delta_ms).round().max(0.0) as u64;
                    st.set_trim_start(next_ms);
                }
            },
        );

    let trim_out_x = (sel_left + sel_w - handle_w).clamp(0.0, track_w - handle_w);
    let trim_out_handle = el()
        .key("trim-out-handle")
        .absolute(trim_out_x, 0.0)
        .size(handle_w, track_h)
        .radius(radius::SM)
        .fill(p.accent)
        .cursor(Cursor::ResizeColumn)
        .on_drag(
            move |st: &mut AppState, d: &mut Drag, _tx: &mut Tx| match d.phase {
                Phase::Down => {
                    d.data = st.editor.trim_end_ms as f32;
                }
                Phase::Move | Phase::Up => {
                    if d.cancelled() {
                        st.set_trim_end(d.data.round().max(0.0) as u64);
                        return;
                    }
                    let delta_ms = (d.dx / track_w) * duration_ms as f32;
                    let next_ms = (d.data + delta_ms).round().max(0.0) as u64;
                    st.set_trim_end(next_ms);
                }
            },
        );

    let mut track = el()
        .key("timeline-track")
        .w(track_w)
        .h(track_h)
        .radius(radius::SM)
        .fill(p.bg_base)
        .border(1.0, p.border_subtle)
        .cursor(Cursor::Pointer)
        .clip_rounded(radius::SM)
        .on_drag(move |st: &mut AppState, d: &mut Drag, _tx: &mut Tx| {
            if d.cancelled() {
                return;
            }
            let ratio = if d.w > 0.0 {
                (d.x / d.w).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let target_ms = (ratio * duration_ms as f32).round() as u64;
            st.seek_editor(target_ms);
        });

    if !s.editor.timeline_thumbs.is_empty() {
        track = track.child(thumb_strip);
    }

    track = track
        .child(
            el().key("timeline-selection")
                .absolute(sel_left, 0.0)
                .size(sel_w, track_h)
                .radius(radius::SM)
                .fill(p.accent_soft)
                .border(1.5, p.accent),
        )
        .child(trim_in_handle)
        .child(trim_out_handle)
        .child(
            el().key("timeline-playhead")
                .absolute(playhead_x, 0.0)
                .size(3.0, track_h)
                .fill(p.text_primary),
        );

    let export_btn = if s.editor.exporting {
        btn_secondary(p, "trim-export", "Exporting...", |_st| {})
    } else {
        btn_primary(p, "trim-export", "Export Clip", |st| {
            st.export_trimmed_clip();
        })
    };

    let timeline_card = card(p, "editor-timeline-card")
        .child(section_header(
            p,
            "timeline-hdr",
            "Trim Timeline",
            "Click or drag timeline to scrub; drag accent handles or use buttons to trim",
        ))
        .child(track)
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
                                format_timecode_ms(s.editor.trim_start_ms)
                            ))
                            .font(type_::BODY)
                            .color(p.text_primary),
                        )
                        .child(pill_option(p, "in-minus", "-1s", false, |st| {
                            let next = st.editor.trim_start_ms.saturating_sub(1_000);
                            st.set_trim_start(next);
                        }))
                        .child(pill_option(p, "in-plus", "+1s", false, |st| {
                            let next = st.editor.trim_start_ms + 1_000;
                            st.set_trim_start(next);
                        }))
                        .child(pill_option(p, "set-in-playhead", "Set In", false, |st| {
                            let pos = st.editor.playhead_ms;
                            st.set_trim_start(pos);
                        })),
                )
                .child(badge(
                    "trim-len",
                    &format!("Selection: {}", format_timecode_ms(selection_ms)),
                    p.accent_text,
                    p.accent_soft,
                ))
                .child(
                    row()
                        .key("trim-out-group")
                        .gap(space::SM)
                        .center_y()
                        .child(
                            text(format!("Out: {}", format_timecode_ms(s.editor.trim_end_ms)))
                                .font(type_::BODY)
                                .color(p.text_primary),
                        )
                        .child(pill_option(p, "out-minus", "-1s", false, |st| {
                            let next = st.editor.trim_end_ms.saturating_sub(1_000);
                            st.set_trim_end(next);
                        }))
                        .child(pill_option(p, "out-plus", "+1s", false, move |st| {
                            let next = (st.editor.trim_end_ms + 1_000).min(duration_ms);
                            st.set_trim_end(next);
                        }))
                        .child(pill_option(p, "set-out-playhead", "Set Out", false, |st| {
                            let pos = st.editor.playhead_ms;
                            st.set_trim_end(pos);
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
                        .center_y()
                        .child(if s.editor.exporting {
                            badge(
                                "exporting-badge",
                                "FFmpeg exporting...",
                                p.accent_text,
                                p.accent_soft,
                            )
                        } else {
                            el().key("exporting-idle")
                        })
                        .child(btn_secondary(p, "trim-reset", "Reset Trim", |st| {
                            st.reset_trim();
                        }))
                        .child(export_btn),
                ),
        );

    let mut root = col()
        .key("editor-view")
        .w_full()
        .gap(space::LG)
        .child(preview_card)
        .child(timeline_card);

    if let Some(err) = &s.editor.export_error {
        root = root.child(
            col()
                .key("export-error-card")
                .padding(space::MD)
                .gap(space::XS)
                .radius(radius::MD)
                .fill(p.recording_soft)
                .border(1.0, p.recording)
                .child(text("Export Failed").font(type_::LABEL).color(p.recording))
                .child(text(err.clone()).font(type_::BODY).color(p.text_primary)),
        );
    }

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
