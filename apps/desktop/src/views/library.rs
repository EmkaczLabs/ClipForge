//! Clip Library view: Grid/List toggle, sort/filter/search, multi-select, trash/restore, rename, and context actions.

use clipforge_common::{ClipId, ClipMetadata};
use clipforge_library::{ClipOrigin, ClipSort};
use rune::tok::type_;
use rune::{Cursor, El, PointerPress, SurfaceFit, TextStyle, Tx, col, el, row, text, text_input};

use crate::components::{
    badge, btn_danger, btn_primary, btn_secondary, card, pill_option, section_header,
};
use crate::state::{
    AppState, ConfirmAction, LibraryFilter, LibraryViewMode, format_bytes, format_duration_ms,
    is_sample_clip,
};
use crate::theme::{Palette, radius, space};

pub fn view(p: &Palette, s: &AppState) -> El {
    let filtered = s.filtered_clips();
    let is_trash_view = s.library_filter == LibraryFilter::Trash;

    let mut root = col()
        .key("library-view")
        .w_full()
        .gap(space::LG)
        .child(section_header(
            p,
            "lib-hdr",
            "Clip Library",
            "Browse, search, sort, multi-select, rename, trash, and manage your clips",
        ))
        .child(toolbar_view(p, s));

    if let Some(confirm) = &s.pending_confirm {
        root = root.child(confirm_bar(p, confirm));
    }

    if !s.selected_clip_ids.is_empty() {
        root = root.child(bulk_bar(p, s, is_trash_view));
    }

    let mut content = col().key("lib-items").w_full().gap(space::MD);
    if let Some(selected) = s.selected_library_clip() {
        content = content.child(selected_detail_view(p, s, selected, is_trash_view));
    }

    if s.using_sample_clips && !is_trash_view {
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
        let empty_title = if is_trash_view {
            "Trash is empty"
        } else {
            "No clips match the active filter"
        };
        let empty_hint = if is_trash_view {
            "Clips moved to Trash appear here until restored or permanently deleted"
        } else {
            "Save a replay, clear your search, or switch back to All Clips"
        };
        content = content.child(
            card(p, "lib-empty")
                .center()
                .py(space::XXL)
                .child(text(empty_title).font(type_::SECTION).color(p.text_primary))
                .child(text(empty_hint).font(type_::LABEL).color(p.text_secondary)),
        );
    } else if s.library_mode == LibraryViewMode::List {
        for clip in filtered {
            content = content.child(list_row_view(p, s, clip, is_trash_view));
        }
    } else {
        for chunk in filtered.chunks(2) {
            let mut r = row()
                .key(format!("grid-row-{}", chunk[0].id))
                .w_full()
                .gap(space::MD);
            for clip in chunk {
                r = r.child(grid_card_view(p, s, clip, is_trash_view));
            }
            content = content.child(r);
        }
    }

    root.child(content)
}

fn toolbar_view(p: &Palette, s: &AppState) -> El {
    let trash_label = format!("Trash ({})", s.trashed_clips.len());
    let mut search_row = row().key("lib-search-row").gap(space::XS).center_y().child(
        text_input(&s.library_search, TextStyle::new(13.0, 400.0))
            .single_line()
            .key("lib-search-input")
            .w(210.0)
            .h(30.0)
            .px(space::SM)
            .py(5.0)
            .radius(radius::SM)
            .fill(p.bg_base)
            .border(1.0, p.border_subtle)
            .placeholder("Search clips...", p.text_muted)
            .on_edit(|st: &mut AppState| &mut st.library_search),
    );
    if !s.library_search.text.is_empty() {
        search_row = search_row.child(btn_secondary(p, "lib-search-clear", "Clear", |st| {
            st.library_search = rune::TextEdit::default();
        }));
    }

    card(p, "lib-toolbar").child(
        col()
            .key("lib-toolbar-col")
            .w_full()
            .gap(space::SM)
            .child(
                row()
                    .key("lib-toolbar-row")
                    .w_full()
                    .flex_wrap()
                    .gap(space::SM)
                    .justify_between()
                    .center_y()
                    .child(
                        row()
                            .key("lib-filters")
                            .flex_wrap()
                            .gap(space::XS)
                            .child(pill_option(
                                p,
                                "filter-all",
                                "All",
                                s.library_filter == LibraryFilter::All,
                                |st| {
                                    st.library_filter = LibraryFilter::All;
                                    st.clear_selection();
                                },
                            ))
                            .child(pill_option(
                                p,
                                "filter-recorded",
                                "Recorded",
                                s.library_filter == LibraryFilter::Recorded,
                                |st| {
                                    st.library_filter = LibraryFilter::Recorded;
                                    st.clear_selection();
                                },
                            ))
                            .child(pill_option(
                                p,
                                "filter-trimmed",
                                "Trimmed",
                                s.library_filter == LibraryFilter::Trimmed,
                                |st| {
                                    st.library_filter = LibraryFilter::Trimmed;
                                    st.clear_selection();
                                },
                            ))
                            .child(pill_option(
                                p,
                                "filter-fav",
                                "Favorites",
                                s.library_filter == LibraryFilter::Favorites,
                                |st| {
                                    st.library_filter = LibraryFilter::Favorites;
                                    st.clear_selection();
                                },
                            ))
                            .child(pill_option(
                                p,
                                "filter-shared",
                                "Shared",
                                s.library_filter == LibraryFilter::Shared,
                                |st| {
                                    st.library_filter = LibraryFilter::Shared;
                                    st.clear_selection();
                                },
                            ))
                            .child(pill_option(
                                p,
                                "filter-trash",
                                &trash_label,
                                s.library_filter == LibraryFilter::Trash,
                                |st| {
                                    st.library_filter = LibraryFilter::Trash;
                                    st.clear_selection();
                                },
                            )),
                    )
                    .child(
                        row()
                            .key("lib-view-toggle")
                            .gap(space::XS)
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
                                    w.send(crate::worker::WorkerCommand::ScanLibrary(
                                        st.config.clone(),
                                    ));
                                } else {
                                    st.status_banner = Some("Scanned clips folder".to_string());
                                }
                            })),
                    ),
            )
            .child(
                row()
                    .key("lib-sort-search-row")
                    .w_full()
                    .flex_wrap()
                    .gap(space::SM)
                    .justify_between()
                    .center_y()
                    .child(search_row)
                    .child(
                        row()
                            .key("lib-sort-pills")
                            .gap(space::XS)
                            .center_y()
                            .child(text("Sort:").font(type_::LABEL).color(p.text_muted))
                            .child(pill_option(
                                p,
                                "sort-newest",
                                "Newest",
                                s.library_sort == ClipSort::CreatedDesc,
                                |st| st.library_sort = ClipSort::CreatedDesc,
                            ))
                            .child(pill_option(
                                p,
                                "sort-oldest",
                                "Oldest",
                                s.library_sort == ClipSort::CreatedAsc,
                                |st| st.library_sort = ClipSort::CreatedAsc,
                            ))
                            .child(pill_option(
                                p,
                                "sort-longest",
                                "Longest",
                                s.library_sort == ClipSort::DurationDesc,
                                |st| st.library_sort = ClipSort::DurationDesc,
                            ))
                            .child(pill_option(
                                p,
                                "sort-largest",
                                "Largest",
                                s.library_sort == ClipSort::SizeDesc,
                                |st| st.library_sort = ClipSort::SizeDesc,
                            ))
                            .child(pill_option(
                                p,
                                "sort-title",
                                "Title",
                                s.library_sort == ClipSort::TitleAsc,
                                |st| st.library_sort = ClipSort::TitleAsc,
                            )),
                    ),
            ),
    )
}

fn confirm_bar(p: &Palette, confirm: &ConfirmAction) -> El {
    let (message, confirm_label, is_danger) = match confirm {
        ConfirmAction::MoveToTrash(ids) => (
            format!(
                "Move {} clip(s) to Trash? You can restore them later from the Trash filter.",
                ids.len()
            ),
            "Confirm Move to Trash",
            false,
        ),
        ConfirmAction::RestoreFromTrash(ids) => (
            format!(
                "Restore {} clip(s) from Trash back to the library?",
                ids.len()
            ),
            "Confirm Restore",
            false,
        ),
        ConfirmAction::PermanentlyDelete(ids) => (
            format!(
                "Permanently delete {} clip(s)? Local recorded/trimmed files will be deleted from disk and cannot be undone.",
                ids.len()
            ),
            "Confirm Permanent Delete",
            true,
        ),
    };

    row()
        .key("lib-confirm-bar")
        .w_full()
        .padding(space::MD)
        .gap(space::MD)
        .justify_between()
        .center_y()
        .radius(radius::MD)
        .fill(if is_danger {
            p.recording_soft
        } else {
            p.warning_soft
        })
        .border(1.0, if is_danger { p.recording } else { p.warning })
        .child(
            text(message)
                .font(type_::BODY)
                .color(p.text_primary)
                .wrap()
                .grow(1.0)
                .min_w(0.0),
        )
        .child(
            row()
                .key("lib-confirm-actions")
                .gap(space::SM)
                .child(btn_secondary(p, "confirm-cancel-btn", "Cancel", |st| {
                    st.cancel_confirmation();
                }))
                .child(if is_danger {
                    btn_danger(p, "confirm-action-btn", confirm_label, |st| {
                        st.confirm_pending_action();
                    })
                } else {
                    btn_primary(p, "confirm-action-btn", confirm_label, |st| {
                        st.confirm_pending_action();
                    })
                }),
        )
}

fn bulk_bar(p: &Palette, s: &AppState, is_trash_view: bool) -> El {
    let count = s.selected_clip_ids.len();
    let selected_vec: Vec<ClipId> = s.selected_clip_ids.iter().copied().collect();
    let fav_ids = selected_vec.clone();
    let trash_ids = selected_vec.clone();
    let restore_ids = selected_vec.clone();
    let del_ids = selected_vec;

    let mut actions = row()
        .key("lib-bulk-actions")
        .flex_wrap()
        .gap(space::XS)
        .child(btn_secondary(p, "bulk-select-all", "Select All", |st| {
            st.select_all_filtered();
        }))
        .child(btn_secondary(p, "bulk-clear", "Clear", |st| {
            st.clear_selection();
        }))
        .child(btn_secondary(p, "bulk-fav", "Bulk Favorite", move |st| {
            st.toggle_favorite_bulk(fav_ids.clone());
        }));

    if is_trash_view {
        actions = actions
            .child(btn_primary(p, "bulk-restore", "Restore", move |st| {
                st.request_restore_from_trash(restore_ids.clone());
            }))
            .child(btn_danger(
                p,
                "bulk-perm-delete",
                "Delete Permanently",
                move |st| st.request_permanent_delete(del_ids.clone()),
            ));
    } else {
        actions = actions
            .child(btn_secondary(p, "bulk-trash", "Move to Trash", move |st| {
                st.request_move_to_trash(trash_ids.clone())
            }))
            .child(btn_danger(
                p,
                "bulk-perm-delete",
                "Delete Permanently",
                move |st| st.request_permanent_delete(del_ids.clone()),
            ));
    }

    row()
        .key("lib-bulk-bar")
        .w_full()
        .flex_wrap()
        .gap(space::SM)
        .padding(space::MD)
        .justify_between()
        .center_y()
        .radius(radius::MD)
        .fill(p.bg_elevated)
        .border(1.0, p.accent)
        .child(
            text(format!(
                "{count} clip{} selected (Ctrl/Shift-click or checkboxes)",
                if count == 1 { "" } else { "s" }
            ))
            .font(type_::LABEL)
            .color(p.text_primary),
        )
        .child(actions)
}

fn origin_badge(p: &Palette, key: &str, origin: ClipOrigin) -> El {
    match origin {
        ClipOrigin::Sample => badge(key, "SAMPLE", p.warning, p.warning_soft),
        ClipOrigin::Trimmed => badge(key, "TRIMMED", p.accent_text, p.accent_soft),
        ClipOrigin::Imported => badge(key, "IMPORTED", p.text_primary, p.bg_elevated),
        ClipOrigin::Recorded => badge(key, "RECORDED", p.success, p.success_soft),
    }
}

fn selected_detail_view(
    p: &Palette,
    s: &AppState,
    selected: &ClipMetadata,
    is_trash_view: bool,
) -> El {
    let sel_id = selected.id;
    let sel_fav = s.favorite_ids.contains(&sel_id);
    let sel_sample = is_sample_clip(selected);
    let sel_origin = s.clip_origin(selected);
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

    let title_area = if s.renaming_clip_id == Some(sel_id) {
        row()
            .key("detail-rename-row")
            .gap(space::XS)
            .center_y()
            .child(
                text_input(&s.rename_edit, TextStyle::new(14.0, 500.0))
                    .single_line()
                    .key("rename-input")
                    .w(260.0)
                    .h(32.0)
                    .px(space::SM)
                    .py(6.0)
                    .radius(radius::SM)
                    .fill(p.bg_base)
                    .border(1.0, p.accent)
                    .placeholder("Enter clip title...", p.text_muted)
                    .on_edit(|st: &mut AppState| &mut st.rename_edit)
                    .on_submit(|st: &mut AppState, _| st.commit_rename()),
            )
            .child(btn_primary(p, "rename-save", "Save", |st| {
                st.commit_rename()
            }))
            .child(btn_secondary(p, "rename-cancel", "Cancel", |st| {
                st.cancel_rename()
            }))
    } else {
        row()
            .key("detail-title-group")
            .gap(space::SM)
            .center_y()
            .child(
                text(selected.title.clone())
                    .font(type_::TITLE)
                    .color(p.text_primary),
            )
            .child(btn_secondary(p, "detail-rename", "Rename", move |st| {
                st.begin_rename(sel_id)
            }))
    };

    let mut actions = row().key("detail-actions").flex_wrap().gap(space::XS);
    if is_trash_view {
        actions = actions
            .child(btn_primary(p, "detail-restore", "Restore", move |st| {
                st.request_restore_from_trash(vec![sel_id]);
            }))
            .child(btn_secondary(
                p,
                "detail-open-folder",
                "Open Folder",
                move |st| st.open_containing_folder(sel_id),
            ))
            .child(btn_danger(
                p,
                "detail-perm-delete",
                "Delete Permanently",
                move |st| st.request_permanent_delete(vec![sel_id]),
            ))
            .child(btn_secondary(p, "detail-menu", "...", move |st| {
                st.open_context_menu(sel_id, 520.0, 260.0);
            }));
    } else {
        actions = actions
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
            .child(btn_primary(p, "detail-edit", "Trim & Edit", move |st| {
                st.open_in_editor(sel_id)
            }))
            .child(btn_secondary(
                p,
                "detail-player",
                "Open in Player",
                move |st| st.open_clip_in_player(sel_id),
            ))
            .child(btn_secondary(
                p,
                "detail-open-folder",
                "Open Folder",
                move |st| st.open_containing_folder(sel_id),
            ))
            .child(btn_secondary(p, "detail-upload", "Upload", move |st| {
                st.queue_upload(sel_id)
            }))
            .child(btn_secondary(p, "detail-trash", "Trash", move |st| {
                st.request_move_to_trash(vec![sel_id]);
            }))
            .child(btn_danger(p, "detail-perm-delete", "Delete", move |st| {
                st.request_permanent_delete(vec![sel_id])
            }))
            .child(btn_secondary(p, "detail-menu", "...", move |st| {
                st.open_context_menu(sel_id, 520.0, 260.0);
            }));
    }

    let trash_info_text = s
        .trashed_at
        .get(&sel_id)
        .map(|dt| format!(" • Trashed {}", dt.format("%Y-%m-%d %H:%M")))
        .unwrap_or_default();

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
                            .child(title_area)
                            .child(origin_badge(p, "detail-origin-badge", sel_origin)),
                    )
                    .child(
                        text(format!(
                            "{sel_game} • {}x{} @ {}fps • {} • {} • {} • {} audio track(s){trash_info_text}",
                            selected.width,
                            selected.height,
                            selected.fps as u32,
                            selected.codec.to_uppercase(),
                            format_duration_ms(selected.duration_ms),
                            format_bytes(selected.size_bytes),
                            selected.audio_tracks.len(),
                        ))
                        .font(type_::LABEL)
                        .color(p.text_secondary),
                    )
                    .child(text(sel_path).font(type_::LABEL).color(p.text_muted))
                    .child(actions),
            ),
    )
}

fn list_row_view(p: &Palette, s: &AppState, clip: &ClipMetadata, is_trash_view: bool) -> El {
    let id = clip.id;
    let fav = s.favorite_ids.contains(&id);
    let is_multi = s.selected_clip_ids.contains(&id);
    let is_selected = is_multi || s.editor.selected_clip_id == Some(id);
    let origin = s.clip_origin(clip);
    let game = clip
        .game
        .as_ref()
        .map(|g| g.name.as_str())
        .unwrap_or("Desktop");

    let mut row_actions = row().key("row-actions").gap(space::XS);
    if is_trash_view {
        row_actions = row_actions
            .child(btn_primary(
                p,
                &format!("restore-{id}"),
                "Restore",
                move |st| {
                    st.request_restore_from_trash(vec![id]);
                },
            ))
            .child(btn_danger(
                p,
                &format!("perm-delete-{id}"),
                "Delete",
                move |st| st.request_permanent_delete(vec![id]),
            ));
    } else {
        row_actions = row_actions
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
                &format!("trash-{id}"),
                "Trash",
                move |st| st.request_move_to_trash(vec![id]),
            ));
    }
    row_actions = row_actions.child(btn_secondary(
        p,
        &format!("menu-btn-{id}"),
        "...",
        move |st| st.open_context_menu(id, 480.0, 260.0),
    ));

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
        .on_press_tx(move |st: &mut AppState, tx: &mut Tx| st.select_clip_with_mods(id, tx.mods()))
        .on_double_press(move |st: &mut AppState| {
            if !is_trash_view {
                st.open_in_editor(id);
            }
        })
        .on_secondary_press(
            move |st: &mut AppState, press: PointerPress, _tx: &mut Tx| {
                st.open_context_menu(id, press.x, press.y);
            },
        )
        .child(
            row()
                .key("list-left")
                .gap(space::SM)
                .center_y()
                .child(pill_option(
                    p,
                    &format!("select-check-{id}"),
                    if is_multi { "✓" } else { "○" },
                    is_multi,
                    move |st| st.toggle_clip_in_selection(id),
                ))
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
                                .child(origin_badge(p, "origin", origin)),
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
                ),
        )
        .child(row_actions)
}

fn grid_card_view(p: &Palette, s: &AppState, clip: &ClipMetadata, is_trash_view: bool) -> El {
    let id = clip.id;
    let fav = s.favorite_ids.contains(&id);
    let is_multi = s.selected_clip_ids.contains(&id);
    let is_selected = is_multi || s.editor.selected_clip_id == Some(id);
    let origin = s.clip_origin(clip);
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
                    .child(origin_badge(p, "origin", origin)),
            )
    };

    let mut card_actions = row().key("card-actions").gap(space::XS);
    if is_trash_view {
        card_actions = card_actions
            .child(btn_primary(
                p,
                &format!("restore-{id}"),
                "Restore",
                move |st| st.request_restore_from_trash(vec![id]),
            ))
            .child(btn_danger(
                p,
                &format!("perm-delete-{id}"),
                "Delete",
                move |st| st.request_permanent_delete(vec![id]),
            ));
    } else {
        card_actions = card_actions
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
                &format!("trash-{id}"),
                "Trash",
                move |st| st.request_move_to_trash(vec![id]),
            ));
    }
    card_actions = card_actions.child(btn_secondary(
        p,
        &format!("menu-btn-{id}"),
        "...",
        move |st| st.open_context_menu(id, 480.0, 260.0),
    ));

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
        .on_press_tx(move |st: &mut AppState, tx: &mut Tx| st.select_clip_with_mods(id, tx.mods()))
        .on_double_press(move |st: &mut AppState| {
            if !is_trash_view {
                st.open_in_editor(id);
            }
        })
        .on_secondary_press(
            move |st: &mut AppState, press: PointerPress, _tx: &mut Tx| {
                st.open_context_menu(id, press.x, press.y);
            },
        )
        .child(thumb_box)
        .child(
            row()
                .key("title-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    row()
                        .key("title-left")
                        .gap(space::XS)
                        .center_y()
                        .child(pill_option(
                            p,
                            &format!("select-check-{id}"),
                            if is_multi { "✓" } else { "○" },
                            is_multi,
                            move |st| st.toggle_clip_in_selection(id),
                        ))
                        .child(
                            text(clip.title.clone())
                                .font(type_::SECTION)
                                .color(p.text_primary),
                        ),
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
        .child(card_actions)
}
