//! Clip Library view: Grid/List toggle, filters, favorites, and quick clip actions.

use rune::tok::type_;
use rune::{Cursor, El, SurfaceFit, col, el, row, text};

use crate::components::{badge, btn_primary, btn_secondary, card, pill_option, section_header};
use crate::state::{
    AppState, LibraryFilter, LibraryViewMode, format_bytes, format_duration_ms, is_sample_clip,
};
use crate::theme::{Palette, radius, space};

pub fn view(p: &Palette, s: &AppState) -> El {
    let filtered = s.filtered_clips();

    let toolbar = card(p, "lib-toolbar").child(
        row()
            .key("lib-toolbar-row")
            .w_full()
            .justify_between()
            .center_y()
            .child(
                row()
                    .key("lib-filters")
                    .gap(space::SM)
                    .child(pill_option(
                        p,
                        "filter-all",
                        "All Clips",
                        s.library_filter == LibraryFilter::All,
                        |st| st.library_filter = LibraryFilter::All,
                    ))
                    .child(pill_option(
                        p,
                        "filter-fav",
                        "Favorites",
                        s.library_filter == LibraryFilter::Favorites,
                        |st| st.library_filter = LibraryFilter::Favorites,
                    ))
                    .child(pill_option(
                        p,
                        "filter-shared",
                        "Shared",
                        s.library_filter == LibraryFilter::Shared,
                        |st| st.library_filter = LibraryFilter::Shared,
                    )),
            )
            .child(
                row()
                    .key("lib-view-toggle")
                    .gap(space::SM)
                    .child(pill_option(
                        p,
                        "mode-grid",
                        "Grid",
                        s.library_mode == LibraryViewMode::Grid,
                        |st| st.library_mode = LibraryViewMode::Grid,
                    ))
                    .child(pill_option(
                        p,
                        "mode-list",
                        "List",
                        s.library_mode == LibraryViewMode::List,
                        |st| st.library_mode = LibraryViewMode::List,
                    ))
                    .child(btn_secondary(p, "lib-scan", "Scan Folder", |st| {
                        if let Some(w) = &st.worker {
                            w.send(crate::worker::WorkerCommand::ScanLibrary(st.config.clone()));
                        } else {
                            st.status_banner = Some("Scanned clips folder".to_string());
                        }
                    })),
            ),
    );

    let mut content = col().key("lib-items").w_full().gap(space::MD);
    if let Some(selected) = s.selected_clip() {
        let sel_id = selected.id;
        let sel_fav = s.favorite_ids.contains(&sel_id);
        let sel_sample = is_sample_clip(selected);
        let sel_game = selected
            .game
            .as_ref()
            .map(|g| g.name.as_str())
            .unwrap_or("Desktop");
        let sel_path = if sel_sample {
            "Sample clip (no file on disk)".to_string()
        } else {
            selected.file_path.display().to_string()
        };
        let preview_thumb = if let Some(px) = s.library_thumbs.get(&sel_id) {
            el().key("detail-thumb-img")
                .w(220.0)
                .h(124.0)
                .radius(radius::MD)
                .fill(p.bg_base)
                .border(1.0, p.border_subtle)
                .pixels_fit(px, SurfaceFit::Fit)
        } else {
            col()
                .key("detail-thumb-placeholder")
                .w(220.0)
                .h(124.0)
                .radius(radius::MD)
                .fill(p.bg_base)
                .border(1.0, p.border_subtle)
                .center()
                .gap(space::XS)
                .child(
                    text(if sel_sample {
                        "Sample Preview"
                    } else {
                        "Clip Preview"
                    })
                    .font(type_::LABEL)
                    .color(p.text_secondary),
                )
                .child(
                    text(format_duration_ms(selected.duration_ms))
                        .font(type_::SECTION)
                        .color(p.text_primary),
                )
        };

        content = content.child(
            card(p, "lib-selected-detail").child(
                row()
                    .key("detail-row")
                    .w_full()
                    .gap(space::LG)
                    .center_y()
                    .child(preview_thumb)
                    .child(
                        col()
                            .key("detail-meta")
                            .grow(1.0)
                            .gap(space::SM)
                            .child(
                                row()
                                    .key("detail-title-row")
                                    .w_full()
                                    .justify_between()
                                    .center_y()
                                    .child(
                                        text(selected.title.clone())
                                            .font(type_::TITLE)
                                            .color(p.text_primary),
                                    )
                                    .child(if sel_sample {
                                        badge(
                                            "detail-sample-badge",
                                            "SAMPLE",
                                            p.warning,
                                            p.warning_soft,
                                        )
                                    } else {
                                        badge(
                                            "detail-rec-badge",
                                            "RECORDED",
                                            p.success,
                                            p.success_soft,
                                        )
                                    }),
                            )
                            .child(
                                text(format!(
                                    "{sel_game} • {}x{} @ {}fps • {} • {} • {}",
                                    selected.width,
                                    selected.height,
                                    selected.fps as u32,
                                    selected.codec.to_uppercase(),
                                    format_duration_ms(selected.duration_ms),
                                    format_bytes(selected.size_bytes),
                                ))
                                .font(type_::LABEL)
                                .color(p.text_secondary),
                            )
                            .child(text(sel_path).font(type_::LABEL).color(p.text_muted))
                            .child(
                                row()
                                    .key("detail-actions")
                                    .gap(space::SM)
                                    .child(pill_option(
                                        p,
                                        "detail-fav",
                                        if sel_fav {
                                            "★ Favorited"
                                        } else {
                                            "☆ Favorite"
                                        },
                                        sel_fav,
                                        move |st| st.toggle_favorite(sel_id),
                                    ))
                                    .child(btn_primary(
                                        p,
                                        "detail-edit",
                                        "Trim & Edit",
                                        move |st| st.open_in_editor(sel_id),
                                    ))
                                    .child(btn_secondary(
                                        p,
                                        "detail-player",
                                        "Open in Player",
                                        move |st| st.open_clip_in_player(sel_id),
                                    ))
                                    .child(btn_secondary(
                                        p,
                                        "detail-upload",
                                        "Upload",
                                        move |st| st.queue_upload(sel_id),
                                    )),
                            ),
                    ),
            ),
        );
    }
    if s.using_sample_clips {
        content = content.child(
            card(p, "lib-sample-notice").child(
                row()
                    .key("lib-sample-row")
                    .w_full()
                    .justify_between()
                    .center_y()
                    .child(
                        text("Showing sample clips — no local recordings indexed yet")
                            .font(type_::LABEL)
                            .color(p.warning),
                    )
                    .child(badge(
                        "lib-sample-badge",
                        "SAMPLE DATA",
                        p.warning,
                        p.warning_soft,
                    )),
            ),
        );
    }
    if filtered.is_empty() {
        content = content.child(
            card(p, "lib-empty")
                .center()
                .py(space::XXL)
                .child(
                    text("No clips match the active filter")
                        .font(type_::SECTION)
                        .color(p.text_primary),
                )
                .child(
                    text("Save a replay or switch back to All Clips")
                        .font(type_::LABEL)
                        .color(p.text_secondary),
                ),
        );
    } else if s.library_mode == LibraryViewMode::List {
        for clip in filtered {
            let id = clip.id;
            let fav = s.favorite_ids.contains(&id);
            let is_selected = s.editor.selected_clip_id == Some(id);
            let game = clip
                .game
                .as_ref()
                .map(|g| g.name.as_str())
                .unwrap_or("Desktop");
            content = content.child(
                row()
                    .key(format!("lib-row-{id}"))
                    .w_full()
                    .padding(space::MD)
                    .justify_between()
                    .center_y()
                    .cursor(Cursor::Pointer)
                    .fill(if is_selected {
                        p.bg_elevated
                    } else {
                        p.bg_surface
                    })
                    .border(
                        if is_selected { 2.0 } else { 1.0 },
                        if is_selected {
                            p.accent
                        } else {
                            p.border_subtle
                        },
                    )
                    .radius(radius::MD)
                    .on_press(move |st: &mut AppState| st.select_clip(id))
                    .on_double_press(move |st: &mut AppState| st.open_in_editor(id))
                    .child(
                        col()
                            .key("info")
                            .gap(4.0)
                            .child(
                                row()
                                    .key("list-title-row")
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
                                    "{game} • {} • {} • {}",
                                    format_duration_ms(clip.duration_ms),
                                    format_bytes(clip.size_bytes),
                                    clip.created_at.format("%Y-%m-%d %H:%M")
                                ))
                                .font(type_::LABEL)
                                .color(p.text_secondary),
                            ),
                    )
                    .child(
                        row()
                            .key("row-actions")
                            .gap(space::SM)
                            .child(pill_option(
                                p,
                                &format!("fav-{id}"),
                                if fav { "★ Fav" } else { "☆ Fav" },
                                fav,
                                move |st| st.toggle_favorite(id),
                            ))
                            .child(btn_primary(p, &format!("edit-{id}"), "Edit", move |st| {
                                st.open_in_editor(id)
                            }))
                            .child(btn_secondary(p, &format!("play-{id}"), "Play", move |st| {
                                st.open_clip_in_player(id)
                            }))
                            .child(btn_secondary(
                                p,
                                &format!("share-{id}"),
                                "Share",
                                move |st| st.queue_upload(id),
                            )),
                    ),
            );
        }
    } else {
        for chunk in filtered.chunks(2) {
            let mut r = row()
                .key(format!("grid-row-{}", chunk[0].id))
                .w_full()
                .gap(space::MD);
            for clip in chunk {
                let id = clip.id;
                let fav = s.favorite_ids.contains(&id);
                let is_selected = s.editor.selected_clip_id == Some(id);
                let game = clip
                    .game
                    .as_ref()
                    .map(|g| g.name.as_str())
                    .unwrap_or("Desktop");
                let thumb_box = if let Some(px) = s.library_thumbs.get(&id) {
                    el().key("thumb")
                        .w_full()
                        .h(110.0)
                        .radius(radius::MD)
                        .fill(p.bg_elevated)
                        .border(1.0, p.border_subtle)
                        .pixels_fit(px, SurfaceFit::Fit)
                } else {
                    el().key("thumb")
                        .w_full()
                        .h(110.0)
                        .radius(radius::MD)
                        .fill(p.bg_elevated)
                        .border(1.0, p.border_subtle)
                        .padding(space::SM)
                        .child(
                            row()
                                .key("thumb-badges")
                                .w_full()
                                .justify_between()
                                .child(badge(
                                    "dur",
                                    &format_duration_ms(clip.duration_ms),
                                    p.text_primary,
                                    p.bg_base,
                                ))
                                .child(if is_sample_clip(clip) {
                                    badge("sample", "SAMPLE", p.warning, p.warning_soft)
                                } else {
                                    badge("recorded", "REC", p.success, p.success_soft)
                                }),
                        )
                };
                r = r.child(
                    card(p, &format!("lib-card-{id}"))
                        .grow(1.0)
                        .cursor(Cursor::Pointer)
                        .fill(if is_selected {
                            p.bg_elevated
                        } else {
                            p.bg_surface
                        })
                        .border(
                            if is_selected { 2.0 } else { 1.0 },
                            if is_selected {
                                p.accent
                            } else {
                                p.border_subtle
                            },
                        )
                        .on_press(move |st: &mut AppState| st.select_clip(id))
                        .on_double_press(move |st: &mut AppState| st.open_in_editor(id))
                        .child(thumb_box)
                        .child(
                            row()
                                .key("title-row")
                                .w_full()
                                .justify_between()
                                .center_y()
                                .child(
                                    text(clip.title.clone())
                                        .font(type_::SECTION)
                                        .color(p.text_primary),
                                )
                                .child(badge(
                                    "codec",
                                    &clip.codec.to_uppercase(),
                                    p.accent_text,
                                    p.accent_soft,
                                )),
                        )
                        .child(
                            text(format!(
                                "{game} • {}x{} @ {}fps • {}",
                                clip.width,
                                clip.height,
                                clip.fps as u32,
                                format_bytes(clip.size_bytes)
                            ))
                            .font(type_::LABEL)
                            .color(p.text_secondary),
                        )
                        .child(
                            row()
                                .key("card-actions")
                                .gap(space::SM)
                                .child(pill_option(
                                    p,
                                    &format!("fav-{id}"),
                                    if fav { "★ Fav" } else { "☆ Fav" },
                                    fav,
                                    move |st| st.toggle_favorite(id),
                                ))
                                .child(btn_primary(
                                    p,
                                    &format!("edit-{id}"),
                                    "Trim & Edit",
                                    move |st| st.open_in_editor(id),
                                ))
                                .child(btn_secondary(
                                    p,
                                    &format!("play-{id}"),
                                    "Player",
                                    move |st| st.open_clip_in_player(id),
                                ))
                                .child(btn_secondary(
                                    p,
                                    &format!("share-{id}"),
                                    "Upload",
                                    move |st| st.queue_upload(id),
                                )),
                        ),
                );
            }
            content = content.child(r);
        }
    }

    col()
        .key("library-view")
        .w_full()
        .gap(space::LG)
        .child(section_header(
            p,
            "lib-hdr",
            "Clip Library",
            "Browse, favorite, trim, and share your captured moments",
        ))
        .child(toolbar)
        .child(content)
}
