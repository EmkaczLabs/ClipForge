//! Video Editor view: Obsidian Forge stage, playback bar, trim timeline, and export inspector.

use rune::{
    Color, Cursor, Drag, El, Key, KeyEvent, Phase, ScrollSpec, SurfaceFit, Tx, col, el, row, text,
};

use crate::components::{
    badge, btn_danger, btn_ghost, btn_primary, btn_secondary, dropdown_select, empty_state,
    error_state, kv_row, loading_state, panel, pill_option, progress_bar, segmented_control,
    segmented_item,
};
use crate::state::{AppState, NavSection, format_bytes, format_duration_ms, is_sample_clip};
use crate::theme::{Palette, radius, space, typography};

/// Formats milliseconds as `MM:SS.cc` for accurate timeline and trim readouts.
fn format_timecode_ms(ms: u64) -> String {
    let total_secs = ms / 1_000;
    let minutes = total_secs / 60;
    let seconds = total_secs % 60;
    let centis = (ms % 1_000) / 10;
    format!("{minutes:02}:{seconds:02}.{centis:02}")
}

/// Renders the full-viewport Obsidian Forge Video Editor workspace.
pub fn view(p: &Palette, s: &AppState) -> El {
    let Some(clip) = s.selected_clip() else {
        return col()
            .key("editor-empty-wrap")
            .w_full()
            .h_full()
            .center()
            .padding(space::XXL)
            .child(el().w(480.0).child(empty_state(
                p,
                "editor-empty",
                "No Clip Selected",
                "Select a clip from your library to preview, trim, and export.",
                Some(btn_primary(p, "go-library", "Browse Clip Library", |st| {
                    st.nav = NavSection::Library;
                })),
            )));
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

    let zoom = s.editor.timeline_zoom.clamp(1.0, 8.0);
    let base_track_w: f32 = 800.0;
    let track_w: f32 = base_track_w * zoom;
    let track_h: f32 = 64.0;
    let handle_w: f32 = 10.0;
    let sel_left = start_ratio * track_w;
    let sel_w = ((end_ratio - start_ratio) * track_w).max(handle_w * 2.0);
    let post_roll_left = (sel_left + sel_w).min(track_w);
    let post_roll_w = (track_w - post_roll_left).max(0.0);
    let playhead_x = (playhead_ratio * track_w).clamp(0.0, track_w - 3.0);

    let res_label = if s.editor.video_width > 0 && s.editor.video_height > 0 {
        format!("{}x{}", s.editor.video_width, s.editor.video_height)
    } else {
        format!("{}x{}", clip.width, clip.height)
    };

    let top_toolbar = build_top_toolbar(p, s, clip, duration_ms, selection_ms, is_sample);
    let video_stage = build_video_stage(
        p,
        s,
        clip,
        &res_label,
        &source_label,
        duration_ms,
        is_sample,
    );
    let playback_bar = build_playback_bar(p, s, clip, clip_id, duration_ms, is_sample);
    let inspector = build_inspector(p, s, clip, &res_label, selection_ms);
    let timeline_dock = build_timeline_dock(
        p,
        s,
        duration_ms,
        zoom,
        track_w,
        track_h,
        handle_w,
        sel_left,
        sel_w,
        post_roll_left,
        post_roll_w,
        playhead_x,
    );

    col()
        .key("editor-view")
        .w_full()
        .h_full()
        .min_h(0.0)
        .fill(p.bg_base)
        .focusable(true)
        .on_key(move |st: &mut AppState, ev: &KeyEvent, _tx: &mut Tx| {
            if ev.mods.ctrl || ev.mods.alt || ev.mods.cmd || ev.mods.logo {
                return false;
            }
            match &ev.key {
                Key::Text(t) if t == " " => {
                    st.toggle_play();
                    true
                }
                Key::Left => {
                    let target = st.editor.playhead_ms.saturating_sub(5_000);
                    st.seek_editor(target);
                    true
                }
                Key::Right => {
                    let target = (st.editor.playhead_ms + 5_000).min(duration_ms);
                    st.seek_editor(target);
                    true
                }
                _ => false,
            }
        })
        .child(top_toolbar)
        .child(
            row()
                .key("editor-middle-split")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .child(
                    col()
                        .key("editor-center-stage")
                        .grow(1.0)
                        .min_w(0.0)
                        .h_full()
                        .min_h(0.0)
                        .padding(space::MD)
                        .gap(space::SM)
                        .child(video_stage)
                        .child(playback_bar),
                )
                .child(inspector),
        )
        .child(timeline_dock)
}

fn build_top_toolbar(
    p: &Palette,
    s: &AppState,
    clip: &clipforge_common::ClipMetadata,
    duration_ms: u64,
    selection_ms: u64,
    is_sample: bool,
) -> El {
    let mut left_cluster = row()
        .key("editor-toolbar-left")
        .gap(space::SM)
        .center_y()
        .child(btn_secondary(p, "back-library", "← Library", |st| {
            st.nav = NavSection::Library;
        }))
        .child(el().w(1.0).h(18.0).fill(p.border_subtle))
        .child(
            text(clip.title.clone())
                .font(typography::HEADLINE_SM)
                .color(p.text_primary),
        );

    if let Some(game) = &clip.game {
        left_cluster = left_cluster.child(badge(
            "editor-game-badge",
            &game.name,
            p.text_secondary,
            p.bg_surface,
        ));
    }

    left_cluster = left_cluster.child(if is_sample {
        badge(
            "editor-sample-badge",
            "SAMPLE",
            p.warning_text,
            p.warning_soft,
        )
    } else {
        badge(
            "editor-rec-badge",
            "RECORDED",
            p.success_text,
            p.success_soft,
        )
    });

    let center_telemetry = row()
        .key("editor-toolbar-telemetry")
        .px(space::MD)
        .py(space::XS)
        .gap(space::SM)
        .center_y()
        .radius(radius::PILL)
        .fill(p.bg_lowest)
        .border(1.0, p.border_subtle)
        .child(
            text(format!("Original: {}", format_timecode_ms(duration_ms)))
                .font(typography::MONO_METRIC)
                .color(p.text_secondary),
        )
        .child(text("•").font(typography::LABEL).color(p.text_muted))
        .child(
            row()
                .px(space::SM)
                .py(2.0)
                .gap(space::XS)
                .center_y()
                .radius(radius::PILL)
                .fill(p.accent_soft)
                .child(
                    el().size(6.0, 6.0)
                        .radius(radius::PILL)
                        .fill(p.accent_primary),
                )
                .child(
                    text(format!("Trim: {}", format_timecode_ms(selection_ms)))
                        .font(typography::MONO_METRIC)
                        .color(p.accent_primary),
                ),
        );

    let right_actions = if s.editor.exporting {
        let pct = (s.editor.export_progress.unwrap_or(0.0).clamp(0.0, 1.0) * 100.0).round() as u32;
        row()
            .key("editor-toolbar-exporting")
            .gap(space::SM)
            .center_y()
            .child(badge(
                "exporting-badge",
                &format!("Exporting {pct}%"),
                p.accent_primary,
                p.accent_soft,
            ))
            .child(btn_danger(p, "cancel-export", "Cancel Export", |st| {
                st.cancel_export();
            }))
    } else {
        row()
            .key("editor-toolbar-actions")
            .gap(space::SM)
            .center_y()
            .child(btn_primary(p, "trim-export", "Export Clip", |st| {
                st.export_trimmed_clip();
            }))
    };

    row()
        .key("editor-top-toolbar")
        .w_full()
        .h(48.0)
        .px(space::MD)
        .justify_between()
        .center_y()
        .fill(p.bg_panel)
        .border_sides([0.0, 0.0, 1.0, 0.0], p.border_subtle)
        .child(left_cluster)
        .child(center_telemetry)
        .child(right_actions)
}

fn build_video_stage(
    p: &Palette,
    s: &AppState,
    clip: &clipforge_common::ClipMetadata,
    res_label: &str,
    source_label: &str,
    duration_ms: u64,
    is_sample: bool,
) -> El {
    let viewport_content = if is_sample {
        col()
            .key("video-sample-placeholder")
            .center()
            .padding(space::XL)
            .child(el().w(440.0).child(empty_state(
                p,
                "video-sample-card",
                "Sample Clip Preview",
                &format!(
                    "Sample clips have no video file on disk. Record or scan a real clip to play video.\n{source_label}"
                ),
                None,
            )))
    } else if let Some(err) = &s.editor.decoder_error {
        col()
            .key("video-decoder-error")
            .center()
            .padding(space::XL)
            .child(el().w(460.0).child(error_state(
                p,
                "video-decoder-error-card",
                "Video Decoder Error",
                err,
                Some(btn_secondary(p, "retry-decode", "Retry Playback", |st| {
                    st.restart_editor();
                })),
            )))
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
            .padding(space::XL)
            .child(el().w(420.0).child(loading_state(
                p,
                "video-loading-card",
                "Decoding Video Frame...",
                source_label,
                None,
            )))
    };

    let hud_overlay = row()
        .key("video-viewport-hud")
        .absolute(0.0, 0.0)
        .w_full()
        .px(space::MD)
        .pt(space::MD)
        .justify_between()
        .center_y()
        .child(
            row()
                .px(space::SM)
                .py(space::XS)
                .gap(space::SM)
                .center_y()
                .radius(radius::XS)
                .fill(p.bg_lowest.alpha(0.85))
                .border(1.0, p.border_subtle)
                .child(el().size(6.0, 6.0).radius(radius::PILL).fill(if is_sample {
                    p.warning
                } else {
                    p.success
                }))
                .child(
                    text(format!(
                        "{res_label} • {}fps • {}",
                        clip.fps as u32,
                        clip.codec.to_uppercase()
                    ))
                    .font(typography::MONO_BADGE)
                    .color(p.text_primary),
                ),
        )
        .child(
            row()
                .px(space::SM)
                .py(space::XS)
                .radius(radius::XS)
                .fill(p.bg_lowest.alpha(0.85))
                .border(1.0, p.border_subtle)
                .child(
                    text(format!(
                        "{} / {}",
                        format_timecode_ms(s.editor.playhead_ms),
                        format_timecode_ms(duration_ms)
                    ))
                    .font(typography::MONO_METRIC)
                    .color(p.text_primary),
                ),
        );

    col()
        .key("video-viewport")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .radius(radius::LG)
        .fill(p.bg_lowest)
        .border(1.0, p.border_subtle)
        .clip_rounded(radius::LG)
        .child(viewport_content)
        .child(hud_overlay)
}

fn build_playback_bar(
    p: &Palette,
    s: &AppState,
    clip: &clipforge_common::ClipMetadata,
    clip_id: clipforge_common::ClipId,
    duration_ms: u64,
    is_sample: bool,
) -> El {
    let slider_w: f32 = 88.0;
    let slider_h: f32 = 20.0;
    let effective_vol = if s.editor.muted {
        0.0
    } else {
        s.editor.volume.clamp(0.0, 1.0)
    };
    let vol_fill_w = (effective_vol * slider_w).clamp(0.0, slider_w);
    let vol_knob_x = (vol_fill_w - 5.0).clamp(0.0, slider_w - 10.0);

    let mut audio_cluster = row()
        .key("audio-controls")
        .gap(space::SM)
        .center_y()
        .child(pill_option(
            p,
            "mute-toggle",
            if s.editor.muted { "Muted" } else { "Mute" },
            s.editor.muted,
            |st| st.toggle_mute(),
        ))
        .child(
            el().key("volume-slider")
                .w(slider_w)
                .h(slider_h)
                .cursor(Cursor::Pointer)
                .on_drag(move |st: &mut AppState, d: &mut Drag, _tx: &mut Tx| {
                    if d.cancelled() {
                        return;
                    }
                    let ratio = if d.w > 0.0 {
                        (d.x / d.w).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    st.set_volume(ratio);
                })
                .child(
                    el().key("volume-track")
                        .absolute(0.0, 7.0)
                        .size(slider_w, 6.0)
                        .radius(radius::PILL)
                        .fill(p.bg_lowest)
                        .border(1.0, p.border_subtle),
                )
                .child(
                    el().key("volume-fill")
                        .absolute(0.0, 7.0)
                        .size(vol_fill_w, 6.0)
                        .radius(radius::PILL)
                        .fill(if s.editor.muted {
                            p.text_muted
                        } else {
                            p.accent_primary
                        }),
                )
                .child(
                    el().key("volume-knob")
                        .absolute(vol_knob_x, 5.0)
                        .size(10.0, 10.0)
                        .radius(radius::PILL)
                        .fill(p.knob),
                ),
        )
        .child(
            text(format!("{}%", (effective_vol * 100.0).round() as u32))
                .font(typography::MONO_METRIC)
                .color(p.text_secondary),
        );

    if clip.audio_tracks.len() > 1 {
        let track_count = clip.audio_tracks.len();
        let current_idx = s.editor.audio_track;
        let current_label = match current_idx {
            None => "Track: Auto".to_string(),
            Some(idx) => {
                let name = clip
                    .audio_tracks
                    .get(idx)
                    .and_then(|t| t.label.as_deref())
                    .unwrap_or("Track");
                format!("T{}: {name}", idx + 1)
            }
        };
        let next_idx = match current_idx {
            None => Some(0),
            Some(i) if i + 1 < track_count => Some(i + 1),
            Some(_) => None,
        };
        audio_cluster = audio_cluster.child(dropdown_select(
            p,
            "audio-track-select",
            "Audio Track",
            &current_label,
            move |st| st.set_audio_track(next_idx),
        ));
    }

    if let Some(_err) = &s.editor.audio_error {
        audio_cluster = audio_cluster.child(badge(
            "audio-err-badge",
            "Audio Off",
            p.warning_text,
            p.warning_soft,
        ));
    } else if !is_sample && s.editor.has_video_frame && !s.editor.has_audio {
        audio_cluster = audio_cluster.child(badge(
            "no-audio-badge",
            "No Audio",
            p.text_muted,
            p.bg_lowest,
        ));
    }

    row()
        .key("playback-controls")
        .w_full()
        .h(48.0)
        .px(space::MD)
        .justify_between()
        .center_y()
        .radius(radius::MD)
        .fill(p.bg_panel)
        .border(1.0, p.border_subtle)
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
                    "set-playhead-start",
                    "Jump to Trim In",
                    |st| {
                        let start = st.editor.trim_start_ms;
                        st.seek_editor(start);
                    },
                )),
        )
        .child(
            row()
                .key("timecode-readout")
                .px(space::SM)
                .py(space::XS)
                .radius(radius::XS)
                .fill(p.bg_lowest)
                .border(1.0, p.border_subtle)
                .child(
                    text(format!(
                        "{} / {}",
                        format_timecode_ms(s.editor.playhead_ms),
                        format_timecode_ms(duration_ms)
                    ))
                    .font(typography::MONO_METRIC)
                    .color(p.text_primary),
                ),
        )
        .child(
            row()
                .key("audio-and-player")
                .gap(space::SM)
                .center_y()
                .child(audio_cluster)
                .child(btn_secondary(
                    p,
                    "open-player",
                    "Open in Player",
                    move |st| st.open_clip_in_player(clip_id),
                )),
        )
}

fn build_inspector(
    p: &Palette,
    s: &AppState,
    clip: &clipforge_common::ClipMetadata,
    res_label: &str,
    selection_ms: u64,
) -> El {
    let mut body = col()
        .key("inspector-body")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .padding(space::LG)
        .gap(space::LG)
        // Segment Bounds
        .child(
            col()
                .key("inspector-bounds")
                .w_full()
                .gap(space::SM)
                .child(
                    row()
                        .w_full()
                        .justify_between()
                        .center_y()
                        .child(
                            text("SEGMENT BOUNDS")
                                .font(typography::MONO_BADGE)
                                .color(p.text_muted),
                        )
                        .child(badge(
                            "trim-len",
                            &format!("Selection {}", format_timecode_ms(selection_ms)),
                            p.accent_primary,
                            p.accent_soft,
                        )),
                )
                .child(
                    row()
                        .w_full()
                        .gap(space::SM)
                        .child(bound_box(
                            p,
                            "bound-in",
                            "Start [In]",
                            format_timecode_ms(s.editor.trim_start_ms),
                        ))
                        .child(bound_box(
                            p,
                            "bound-out",
                            "End [Out]",
                            format_timecode_ms(s.editor.trim_end_ms),
                        )),
                ),
        )
        // Clip Metadata
        .child(
            col()
                .key("inspector-metadata")
                .w_full()
                .gap(space::XS)
                .child(
                    text("CLIP METADATA")
                        .font(typography::MONO_BADGE)
                        .color(p.text_muted),
                )
                .child(kv_row(
                    p,
                    "meta-res",
                    "Resolution",
                    format!("{res_label} @ {}fps", clip.fps as u32),
                ))
                .child(kv_row(p, "meta-codec", "Codec", clip.codec.to_uppercase()))
                .child(kv_row(
                    p,
                    "meta-dur",
                    "Duration",
                    format_duration_ms(clip.duration_ms),
                ))
                .child(kv_row(
                    p,
                    "meta-size",
                    "File Size",
                    format_bytes(clip.size_bytes),
                ))
                .child(kv_row(
                    p,
                    "meta-game",
                    "Game",
                    clip.game
                        .as_ref()
                        .map(|g| g.name.clone())
                        .unwrap_or_else(|| "None".to_string()),
                ))
                .child(kv_row(
                    p,
                    "meta-audio",
                    "Audio",
                    if clip.audio_tracks.is_empty() {
                        if s.editor.has_audio {
                            "1 track".to_string()
                        } else {
                            "None".to_string()
                        }
                    } else {
                        format!("{} track(s)", clip.audio_tracks.len())
                    },
                )),
        );

    if clip.audio_tracks.len() > 1 {
        let mut track_pills = row()
            .key("inspector-audio-tracks")
            .w_full()
            .gap(space::XS)
            .wrap()
            .child(pill_option(
                p,
                "track-auto",
                "Auto",
                s.editor.audio_track.is_none(),
                |st| st.set_audio_track(None),
            ));
        for (idx, track) in clip.audio_tracks.iter().enumerate() {
            let label = track
                .label
                .clone()
                .unwrap_or_else(|| format!("Track {}", idx + 1));
            track_pills = track_pills.child(pill_option(
                p,
                &format!("track-{idx}"),
                &label,
                s.editor.audio_track == Some(idx),
                move |st| st.set_audio_track(Some(idx)),
            ));
        }
        body = body.child(
            col()
                .key("inspector-audio-section")
                .w_full()
                .gap(space::SM)
                .child(
                    text("AUDIO TRACK")
                        .font(typography::MONO_BADGE)
                        .color(p.text_muted),
                )
                .child(track_pills),
        );
    }

    // Export Options
    body = body.child(
        col()
            .key("inspector-export-options")
            .w_full()
            .gap(space::SM)
            .child(
                text("EXPORT MODE")
                    .font(typography::MONO_BADGE)
                    .color(p.text_muted),
            )
            .child(segmented_control(
                p,
                "reencode-switch",
                vec![
                    segmented_item(
                        p,
                        "mode-stream-copy",
                        "Fast Trim",
                        !s.editor.reencode,
                        |st| {
                            st.editor.reencode = false;
                        },
                    ),
                    segmented_item(
                        p,
                        "mode-reencode",
                        "Precise Trim",
                        s.editor.reencode,
                        |st| {
                            st.editor.reencode = true;
                        },
                    ),
                ],
            ))
            .child(
                text(if s.editor.reencode {
                    "Precise Trim (Re-encode) — re-encodes the trimmed segment for exact frame-accurate cuts."
                } else {
                    "Fast Trim (Stream Copy) — instant lossless copy; start cut aligns to the nearest keyframe."
                })
                .font(typography::LABEL)
                .color(p.text_muted)
                .w_full()
                .wrap(),
            ),
    );

    if s.editor.exporting {
        let progress = s.editor.export_progress.unwrap_or(0.0).clamp(0.0, 1.0);
        let pct = (progress * 100.0).round() as u32;
        body = body.child(
            col()
                .key("export-progress-box")
                .w_full()
                .padding(space::MD)
                .gap(space::SM)
                .radius(radius::MD)
                .fill(p.bg_surface)
                .border(1.0, p.border_subtle)
                .child(
                    row()
                        .key("export-progress-hdr")
                        .w_full()
                        .justify_between()
                        .center_y()
                        .child(
                            text(format!("Exporting... {pct}%"))
                                .font(typography::LABEL)
                                .color(p.text_primary),
                        )
                        .child(
                            text(if s.editor.reencode {
                                "Precise Trim (Re-encode)"
                            } else {
                                "Fast Trim (Stream Copy)"
                            })
                            .font(typography::MICRO)
                            .color(p.text_secondary),
                        ),
                )
                .child(progress_bar(
                    p,
                    "export-progress-track",
                    progress,
                    p.accent_primary,
                )),
        );
    }

    if let Some(err) = &s.editor.export_error {
        body = body.child(
            col()
                .key("export-error-card")
                .w_full()
                .padding(space::MD)
                .gap(space::XS)
                .radius(radius::MD)
                .fill(p.recording_soft)
                .border(1.0, p.recording)
                .child(
                    text("Export Failed")
                        .font(typography::LABEL)
                        .color(p.error_text),
                )
                .child(
                    text(err.clone())
                        .font(typography::BODY)
                        .color(p.text_primary),
                ),
        );
    }

    if let Some(export_path) = &s.editor.last_export_path {
        let file_name = export_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| export_path.display().to_string());
        body = body.child(
            col()
                .key("export-result-card")
                .w_full()
                .padding(space::MD)
                .gap(space::XS)
                .radius(radius::MD)
                .fill(p.bg_surface)
                .border(1.0, p.border_subtle)
                .child(
                    text("LAST EXPORT OUTPUT")
                        .font(typography::MONO_BADGE)
                        .color(p.success_text),
                )
                .child(text(file_name).font(typography::BODY).color(p.text_primary))
                .child(
                    text(export_path.display().to_string())
                        .font(typography::MICRO)
                        .color(p.text_muted),
                ),
        );
    }

    let footer_cta = if s.editor.exporting {
        btn_danger(p, "inspector-cancel-export", "Cancel Export", |st| {
            st.cancel_export();
        })
        .w_full()
    } else {
        btn_primary(p, "inspector-export-clip", "Export Clip", |st| {
            st.export_trimmed_clip();
        })
        .w_full()
    };

    panel(p, "editor-inspector")
        .w(320.0)
        .h_full()
        .border_sides([0.0, 0.0, 0.0, 1.0], p.border_subtle)
        .child(
            row()
                .key("editor-inspector-header")
                .w_full()
                .h(44.0)
                .px(space::LG)
                .center_y()
                .justify_between()
                .border_sides([0.0, 0.0, 1.0, 0.0], p.border_subtle)
                .child(
                    text("CLIP & EXPORT")
                        .font(typography::MONO_BADGE)
                        .color(p.text_muted),
                )
                .child(btn_ghost(p, "trim-reset", "Reset Trim", |st| {
                    st.reset_trim();
                })),
        )
        .child(body)
        .child(
            row()
                .key("editor-inspector-footer")
                .w_full()
                .padding(space::MD)
                .border_sides([1.0, 0.0, 0.0, 0.0], p.border_subtle)
                .child(footer_cta),
        )
}

fn bound_box(p: &Palette, key: &str, label_text: &str, value_text: String) -> El {
    col()
        .key(key)
        .grow(1.0)
        .padding(space::SM)
        .gap(space::XS)
        .radius(radius::SM)
        .fill(p.bg_surface)
        .border(1.0, p.border_subtle)
        .child(
            text(label_text.to_string())
                .font(typography::MICRO)
                .color(p.text_muted),
        )
        .child(
            text(value_text)
                .font(typography::MONO_METRIC)
                .color(p.text_primary),
        )
}

#[allow(clippy::too_many_arguments)]
fn build_timeline_dock(
    p: &Palette,
    s: &AppState,
    duration_ms: u64,
    zoom: f32,
    track_w: f32,
    track_h: f32,
    handle_w: f32,
    sel_left: f32,
    sel_w: f32,
    post_roll_left: f32,
    post_roll_w: f32,
    playhead_x: f32,
) -> El {
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
        .radius(radius::XS)
        .fill(p.accent_primary)
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
        .radius(radius::XS)
        .fill(p.accent_primary)
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
        .fill(p.bg_lowest)
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

    if sel_left > 0.5 {
        track = track.child(
            el().key("timeline-dim-pre")
                .absolute(0.0, 0.0)
                .size(sel_left, track_h)
                .fill(Color::BLACK.alpha(0.55)),
        );
    }

    if post_roll_w > 0.5 {
        track = track.child(
            el().key("timeline-dim-post")
                .absolute(post_roll_left, 0.0)
                .size(post_roll_w, track_h)
                .fill(Color::BLACK.alpha(0.55)),
        );
    }

    track = track
        .child(
            el().key("timeline-selection")
                .absolute(sel_left, 0.0)
                .size(sel_w, track_h)
                .radius(radius::XS)
                .fill(p.accent_soft)
                .border(1.5, p.accent_primary),
        )
        .child(trim_in_handle)
        .child(trim_out_handle)
        .child(
            el().key("timeline-playhead")
                .absolute(playhead_x, 0.0)
                .size(3.0, track_h)
                .fill(p.text_primary),
        );

    let mut zoom_controls = row()
        .key("timeline-zoom-controls")
        .gap(space::XS)
        .center_y()
        .child(pill_option(p, "zoom-out", "-", false, |st| {
            let next = (st.editor.timeline_zoom * 0.5).max(1.0);
            st.set_timeline_zoom(next);
        }))
        .child(pill_option(
            p,
            "zoom-reset",
            "1x",
            (zoom - 1.0).abs() < 0.05,
            |st| st.set_timeline_zoom(1.0),
        ))
        .child(pill_option(p, "zoom-in", "+", false, |st| {
            let next = (st.editor.timeline_zoom * 2.0).min(8.0);
            st.set_timeline_zoom(next);
        }));

    if zoom > 1.05 {
        zoom_controls = zoom_controls.child(badge(
            "zoom-level-badge",
            &format!("{zoom:.0}x"),
            p.accent_primary,
            p.accent_soft,
        ));
    }

    col()
        .key("editor-timeline-card")
        .w_full()
        .padding(space::MD)
        .gap(space::SM)
        .fill(p.bg_panel)
        .border_sides([1.0, 0.0, 0.0, 0.0], p.border_subtle)
        .child(
            row()
                .key("timeline-hdr-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    row()
                        .key("timeline-timecode-cluster")
                        .gap(space::SM)
                        .center_y()
                        .child(
                            row()
                                .px(space::SM)
                                .py(2.0)
                                .radius(radius::XS)
                                .fill(p.bg_lowest)
                                .border(1.0, p.border_subtle)
                                .child(
                                    text(format_timecode_ms(s.editor.playhead_ms))
                                        .font(typography::MONO_METRIC)
                                        .color(p.accent_primary),
                                ),
                        )
                        .child(
                            text(format!("DURATION {}", format_timecode_ms(duration_ms)))
                                .font(typography::MONO_BADGE)
                                .color(p.text_muted),
                        ),
                )
                .child(
                    row()
                        .key("trim-controls")
                        .gap(space::MD)
                        .center_y()
                        .child(
                            row()
                                .key("trim-in-group")
                                .gap(space::XS)
                                .center_y()
                                .child(
                                    text(format!(
                                        "In {}",
                                        format_timecode_ms(s.editor.trim_start_ms)
                                    ))
                                    .font(typography::MONO_METRIC)
                                    .color(p.text_secondary),
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
                        .child(el().w(1.0).h(16.0).fill(p.border_subtle))
                        .child(
                            row()
                                .key("trim-out-group")
                                .gap(space::XS)
                                .center_y()
                                .child(
                                    text(format!(
                                        "Out {}",
                                        format_timecode_ms(s.editor.trim_end_ms)
                                    ))
                                    .font(typography::MONO_METRIC)
                                    .color(p.text_secondary),
                                )
                                .child(pill_option(p, "out-minus", "-1s", false, |st| {
                                    let next = st.editor.trim_end_ms.saturating_sub(1_000);
                                    st.set_trim_end(next);
                                }))
                                .child(pill_option(p, "out-plus", "+1s", false, move |st| {
                                    let next = (st.editor.trim_end_ms + 1_000).min(duration_ms);
                                    st.set_trim_end(next);
                                }))
                                .child(pill_option(
                                    p,
                                    "set-out-playhead",
                                    "Set Out",
                                    false,
                                    |st| {
                                        let pos = st.editor.playhead_ms;
                                        st.set_trim_end(pos);
                                    },
                                )),
                        ),
                )
                .child(zoom_controls),
        )
        .child(
            row()
                .key("timeline-scroll")
                .w_full()
                .scroll_x()
                .child(track),
        )
}
