//! Clip Library view: Obsidian Forge Grid/List browser, game/origin filters, multi-select,
//! inline rename, confirmation bar, context menu, and right-hand Clip Inspector.

use std::collections::HashMap;

use clipforge_common::{ClipId, ClipMetadata};
use clipforge_library::{ClipOrigin, ClipSort};
use rune::{Color, Cursor, El, PointerPress, SurfaceFit, Tx, col, el, icon, row, text, text_input};

use crate::components::{
    badge, btn_danger, btn_ghost, btn_primary, btn_secondary, checkbox, clip_card_frame,
    empty_state, icon_btn_sized, icons, inspector_panel, kv_row, pill_option, segmented_control,
    segmented_item,
};
use crate::state::{
    AppState, ConfirmAction, LibraryFilter, LibraryViewMode, format_bytes, format_duration_ms,
    is_sample_clip,
};
use crate::theme::{Palette, radius, snappy_spring, space, typography};

/// Renders the Obsidian Forge Clip Library workspace.
pub fn view(p: &Palette, s: &AppState) -> El {
    let filtered = s.filtered_clips();
    let is_trash_view = s.library_filter == LibraryFilter::Trash;
    let show_inspector = s.inspector_open && s.selected_library_clip().is_some();

    let mut root = col()
        .key("library-view")
        .w_full()
        .gap(space::MD)
        .child(header_row(p, s, is_trash_view))
        .child(toolbar_view(p, s));

    if let Some(confirm) = &s.pending_confirm {
        root = root.child(confirm_bar(p, confirm));
    }

    if !s.selected_clip_ids.is_empty() {
        root = root.child(bulk_bar(p, s, is_trash_view));
    }

    let grid_or_list = build_clips_pane(p, s, &filtered, is_trash_view, show_inspector);

    let mut workspace = row()
        .key("lib-workspace")
        .w_full()
        .gap(space::LG)
        .child(grid_or_list);

    if let Some(selected) = s.selected_library_clip().filter(|_| s.inspector_open) {
        workspace = workspace.child(inspector_view(p, s, selected, is_trash_view));
    }

    root.child(workspace)
}

fn header_row(p: &Palette, s: &AppState, is_trash_view: bool) -> El {
    let active_count = s.clips.len();
    let total_bytes: u64 = s.clips.iter().map(|c| c.size_bytes).sum();
    let summary = format!(
        "{active_count} clip{} • {}",
        if active_count == 1 { "" } else { "s" },
        format_bytes(total_bytes)
    );

    let mut left = row()
        .key("lib-header-left")
        .gap(space::MD)
        .center_y()
        .child(
            text("Clip Library")
                .font(typography::HEADLINE_LG)
                .color(p.text_primary),
        )
        .child(badge(
            "lib-summary-pill",
            &summary,
            p.text_secondary,
            p.bg_surface,
        ));

    if s.using_sample_clips && !is_trash_view {
        left = left.child(badge(
            "lib-sample-badge",
            "SAMPLE DATA",
            p.warning_text,
            p.warning_soft,
        ));
    }

    let mut right = row().key("lib-header-right").gap(space::SM).center_y();
    if s.selected_library_clip().is_some() {
        right = right.child(pill_option(
            p,
            "lib-inspector-toggle",
            if s.inspector_open {
                "Inspector On"
            } else {
                "Inspector"
            },
            s.inspector_open,
            |st| st.inspector_open = !st.inspector_open,
        ));
    }
    right = right.child(btn_secondary(p, "lib-scan", "Scan Folder", |st| {
        if let Some(w) = &st.worker {
            w.send(crate::worker::WorkerCommand::ScanLibrary(st.config.clone()));
        } else {
            st.status_banner = Some("Scanned clips folder".to_string());
        }
    }));

    row()
        .key("lib-hdr")
        .w_full()
        .justify_between()
        .center_y()
        .child(left)
        .child(right)
}

fn toolbar_view(p: &Palette, s: &AppState) -> El {
    let trash_label = format!("Trash ({})", s.trashed_clips.len());

    let mode_switch = segmented_control(
        p,
        "lib-view-toggle",
        vec![
            segmented_item(
                p,
                "mode-grid",
                "Grid",
                s.library_mode == LibraryViewMode::Grid,
                |st| st.library_mode = LibraryViewMode::Grid,
            ),
            segmented_item(
                p,
                "mode-list",
                "List",
                s.library_mode == LibraryViewMode::List,
                |st| st.library_mode = LibraryViewMode::List,
            ),
        ],
    );

    let filter_pills = row()
        .key("lib-filters")
        .flex_wrap()
        .gap(space::XS)
        .center_y()
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
        ));

    let mut search_row = row().key("lib-search-row").gap(space::XS).center_y().child(
        row()
            .key("lib-search-box")
            .w(220.0)
            .h(32.0)
            .px(space::SM)
            .gap(space::XS)
            .center_y()
            .radius(radius::XS)
            .fill(p.bg_lowest)
            .border(1.0, p.border_subtle)
            .focus_ring(1.0, 0.0, p.border_focus)
            .child(icon(&icons::SEARCH, 14.0).color(p.text_muted).a11y_hidden())
            .child(
                text_input(&s.library_search, typography::BODY)
                    .single_line()
                    .key("lib-search-input")
                    .label("Search clips")
                    .grow(1.0)
                    .min_w(0.0)
                    .color(p.text_primary)
                    .placeholder("Filter title, game, tag...", p.text_muted)
                    .on_edit(|st: &mut AppState| &mut st.library_search),
            ),
    );
    if !s.library_search.text.is_empty() {
        search_row = search_row.child(btn_ghost(p, "lib-search-clear", "Clear", |st| {
            st.library_search = rune::TextEdit::default();
        }));
    }

    let sort_pills = row()
        .key("lib-sort-pills")
        .gap(space::XS)
        .center_y()
        .child(text("Sort:").font(typography::LABEL).color(p.text_muted))
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
        ));

    let mut toolbar = col()
        .key("lib-toolbar-col")
        .w_full()
        .gap(space::SM)
        .padding(space::MD)
        .fill(p.bg_panel)
        .radius(radius::MD)
        .border(1.0, p.border_subtle)
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
                        .key("lib-toolbar-left")
                        .flex_wrap()
                        .gap(space::SM)
                        .center_y()
                        .child(mode_switch)
                        .child(filter_pills),
                )
                .child(
                    row()
                        .key("lib-toolbar-right")
                        .flex_wrap()
                        .gap(space::SM)
                        .center_y()
                        .child(search_row)
                        .child(sort_pills),
                ),
        );

    let games = s.available_games();
    if !games.is_empty() && s.library_filter != LibraryFilter::Trash {
        let mut game_counts: HashMap<&str, usize> = HashMap::new();
        for clip in &s.clips {
            if let Some(g) = &clip.game {
                *game_counts.entry(g.name.as_str()).or_insert(0) += 1;
            }
        }
        let mut game_row = row()
            .key("lib-game-filters")
            .w_full()
            .flex_wrap()
            .gap(space::XS)
            .center_y()
            .child(pill_option(
                p,
                "game-filter-all",
                &format!("All Games ({})", s.clips.len()),
                s.library_game_filter.is_none(),
                |st| st.set_library_game_filter(None),
            ));

        for (idx, game_name) in games.into_iter().enumerate() {
            let count = game_counts.get(game_name.as_str()).copied().unwrap_or(0);
            let active = s.library_game_filter.as_deref() == Some(game_name.as_str());
            let label = format!("{game_name} ({count})");
            let target_game = game_name.clone();
            game_row = game_row.child(pill_option(
                p,
                &format!("game-filter-{idx}"),
                &label,
                active,
                move |st| st.set_library_game_filter(Some(target_game.clone())),
            ));
        }
        toolbar = toolbar.child(game_row);
    }

    toolbar
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
                .font(typography::BODY)
                .color(if is_danger {
                    p.error_text
                } else {
                    p.text_primary
                })
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
    let selected_bytes: u64 = s
        .clips
        .iter()
        .chain(s.trashed_clips.iter())
        .filter(|c| s.selected_clip_ids.contains(&c.id))
        .map(|c| c.size_bytes)
        .sum();
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
        .border(1.0, p.accent_brand)
        .child(
            row()
                .key("lib-bulk-info")
                .gap(space::SM)
                .center_y()
                .child(
                    text(format!(
                        "{count} clip{} selected",
                        if count == 1 { "" } else { "s" }
                    ))
                    .font(typography::LABEL)
                    .color(p.text_primary),
                )
                .child(badge(
                    "bulk-size-pill",
                    &format_bytes(selected_bytes),
                    p.text_secondary,
                    p.bg_lowest,
                ))
                .child(
                    text("Ctrl/Shift-click or checkboxes")
                        .font(typography::MICRO)
                        .color(p.text_muted),
                ),
        )
        .child(actions)
}

fn build_clips_pane(
    p: &Palette,
    s: &AppState,
    filtered: &[&ClipMetadata],
    is_trash_view: bool,
    show_inspector: bool,
) -> El {
    let mut pane = col().key("lib-items").grow(1.0).min_w(0.0).gap(space::MD);

    if filtered.is_empty() {
        let has_active_filter = !s.library_search.text.is_empty()
            || s.library_game_filter.is_some()
            || s.library_filter != LibraryFilter::All;

        let empty_title = if is_trash_view {
            "Trash is empty"
        } else {
            "No clips match the active filter"
        };
        let empty_hint = if is_trash_view {
            "Clips moved to Trash appear here until restored or permanently deleted."
        } else if has_active_filter {
            "Try clearing your search query, game filter, or category tab."
        } else {
            "Save a replay or start a recording in Recording Studio to populate your library."
        };

        let cta = if has_active_filter {
            Some(btn_primary(p, "lib-clear-filters", "Clear Filters", |st| {
                st.library_filter = LibraryFilter::All;
                st.library_game_filter = None;
                st.library_search = rune::TextEdit::default();
            }))
        } else if !is_trash_view {
            Some(btn_primary(
                p,
                "lib-go-recording",
                "Go to Recording Studio",
                |st| st.nav = crate::state::NavSection::Recording,
            ))
        } else {
            None
        };

        return pane.child(empty_state(p, "lib-empty", empty_title, empty_hint, cta));
    }

    if s.library_mode == LibraryViewMode::List {
        for clip in filtered {
            pane = pane.child(list_row_view(p, s, clip, is_trash_view));
        }
    } else {
        let cols = if show_inspector { 2 } else { 3 };
        for chunk in filtered.chunks(cols) {
            let mut r = row()
                .key(format!("grid-row-{}", chunk[0].id))
                .w_full()
                .gap(space::MD);
            for clip in chunk {
                r = r.child(grid_card_view(p, s, clip, is_trash_view));
            }
            // Pad incomplete final row so cards keep uniform column widths.
            for idx in chunk.len()..cols {
                r = r.child(
                    el().key(format!("grid-spacer-{}-{idx}", chunk[0].id))
                        .grow(1.0)
                        .min_w(0.0),
                );
            }
            pane = pane.child(r);
        }
    }

    pane
}

fn origin_badge(p: &Palette, key: &str, origin: ClipOrigin) -> El {
    match origin {
        ClipOrigin::Sample => badge(key, "SAMPLE", p.warning_text, p.warning_soft),
        ClipOrigin::Trimmed => badge(key, "TRIMMED", p.on_accent_primary, p.accent_primary),
        ClipOrigin::Imported => badge(key, "IMPORTED", p.text_primary, p.bg_elevated),
        ClipOrigin::Recorded => badge(key, "RECORDED", p.success_text, p.success_soft),
    }
}

fn inspector_view(p: &Palette, s: &AppState, selected: &ClipMetadata, is_trash_view: bool) -> El {
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
            .w_full()
            .h(162.0)
            .radius(radius::SM)
            .fill(p.bg_lowest)
            .border(1.0, p.border_subtle)
            .pixels_fit(px, SurfaceFit::Fit)
    } else {
        col()
            .key("detail-thumb-placeholder")
            .w_full()
            .h(162.0)
            .radius(radius::SM)
            .fill(p.bg_lowest)
            .border(1.0, p.border_subtle)
            .center()
            .gap(space::XS)
            .child(
                text(if sel_sample {
                    "SAMPLE PREVIEW"
                } else {
                    "CLIP PREVIEW"
                })
                .font(typography::MONO_BADGE)
                .color(p.text_muted),
            )
            .child(
                text(format_duration_ms(selected.duration_ms))
                    .font(typography::HEADLINE_MD)
                    .color(p.text_primary),
            )
    };

    let title_area = if s.renaming_clip_id == Some(sel_id) {
        col()
            .key("detail-rename-col")
            .w_full()
            .gap(space::XS)
            .child(
                text_input(&s.rename_edit, typography::BODY)
                    .single_line()
                    .key("rename-input")
                    .label("Rename clip title")
                    .w_full()
                    .h(32.0)
                    .px(space::SM)
                    .py(6.0)
                    .radius(radius::XS)
                    .fill(p.bg_lowest)
                    .border(1.0, p.border_focus)
                    .color(p.text_primary)
                    .placeholder("Enter clip title...", p.text_muted)
                    .on_edit(|st: &mut AppState| &mut st.rename_edit)
                    .on_submit(|st: &mut AppState, _| st.commit_rename()),
            )
            .child(
                row()
                    .key("detail-rename-btns")
                    .gap(space::XS)
                    .child(btn_primary(p, "rename-save", "Save", |st| {
                        st.commit_rename()
                    }))
                    .child(btn_secondary(p, "rename-cancel", "Cancel", |st| {
                        st.cancel_rename()
                    })),
            )
    } else {
        row()
            .key("detail-title-group")
            .w_full()
            .justify_between()
            .center_y()
            .gap(space::SM)
            .child(
                text(selected.title.clone())
                    .font(typography::HEADLINE_SM)
                    .color(p.text_primary)
                    .wrap()
                    .grow(1.0)
                    .min_w(0.0),
            )
            .child(btn_ghost(p, "detail-rename", "Rename", move |st| {
                st.begin_rename(sel_id)
            }))
    };

    let mut badges_row = row()
        .key("detail-badges-row")
        .w_full()
        .flex_wrap()
        .gap(space::XS)
        .child(origin_badge(p, "detail-origin-badge", sel_origin));
    for (idx, tag) in selected.tags.iter().enumerate() {
        badges_row = badges_row.child(badge(
            &format!("detail-tag-{idx}"),
            &format!("#{tag}"),
            p.text_secondary,
            p.bg_surface,
        ));
    }

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
            .child(btn_primary(p, "detail-edit", "Trim & Edit", move |st| {
                st.open_in_editor(sel_id)
            }))
            .child(btn_secondary(
                p,
                "detail-play-editor",
                "Play in Editor",
                move |st| st.play_in_editor(sel_id),
            ))
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

    let mut meta_col = col()
        .key("detail-kv-list")
        .w_full()
        .gap(2.0)
        .pt(space::SM)
        .border_sides([1.0, 0.0, 0.0, 0.0], p.border_subtle)
        .child(kv_row(p, "kv-game", "Game", sel_game.to_string()))
        .child(kv_row(
            p,
            "kv-codec",
            "Codec",
            selected.codec.to_uppercase(),
        ))
        .child(kv_row(
            p,
            "kv-res",
            "Resolution",
            format!(
                "{}x{} @ {}fps",
                selected.width, selected.height, selected.fps as u32
            ),
        ))
        .child(kv_row(
            p,
            "kv-dur",
            "Duration",
            format_duration_ms(selected.duration_ms),
        ))
        .child(kv_row(
            p,
            "kv-size",
            "File Size",
            format_bytes(selected.size_bytes),
        ))
        .child(kv_row(
            p,
            "kv-audio",
            "Audio Tracks",
            format!("{} track(s)", selected.audio_tracks.len()),
        ))
        .child(kv_row(
            p,
            "kv-created",
            "Created",
            selected.created_at.format("%Y-%m-%d %H:%M").to_string(),
        ));

    if let Some(dt) = s.trashed_at.get(&sel_id) {
        meta_col = meta_col.child(kv_row(
            p,
            "kv-trashed",
            "Trashed",
            dt.format("%Y-%m-%d %H:%M").to_string(),
        ));
    }

    let path_box = col()
        .key("detail-path-box")
        .w_full()
        .gap(space::XS)
        .padding(space::SM)
        .radius(radius::SM)
        .fill(p.bg_lowest)
        .border(1.0, p.border_subtle)
        .child(
            text("LOCAL SYSTEM PATH")
                .font(typography::MICRO)
                .color(p.text_muted),
        )
        .child(
            text(sel_path)
                .font(typography::MONO_BADGE)
                .color(p.text_secondary)
                .wrap(),
        );

    let body = col()
        .key("detail-body")
        .w_full()
        .padding(space::LG)
        .gap(space::MD)
        .child(preview_thumb)
        .child(title_area)
        .child(badges_row)
        .child(actions)
        .child(meta_col)
        .child(path_box);

    inspector_panel(
        p,
        "lib-selected-detail",
        "INSPECTOR • FOCUSED CLIP",
        |st| st.inspector_open = false,
        body,
        None,
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

    let thumb = if let Some(px) = s.library_thumbs.get(&id) {
        el().key(format!("list-thumb-{id}"))
            .w(88.0)
            .h(50.0)
            .radius(radius::XS)
            .fill(p.bg_lowest)
            .border(1.0, p.border_subtle)
            .pixels_fit(px, SurfaceFit::Fit)
    } else {
        el().key(format!("list-thumb-{id}"))
            .w(88.0)
            .h(50.0)
            .radius(radius::XS)
            .fill(p.bg_lowest)
            .border(1.0, p.border_subtle)
            .center()
            .child(
                text(format_duration_ms(clip.duration_ms))
                    .font(typography::MONO_BADGE)
                    .color(p.text_secondary),
            )
    };

    let mut row_actions = row().key("row-actions").gap(space::XS).center_y();
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
                st.play_in_editor(id)
            }))
            .child(btn_secondary(
                p,
                &format!("trash-{id}"),
                "Trash",
                move |st| st.request_move_to_trash(vec![id]),
            ));
    }
    row_actions = row_actions.child(icon_btn_sized(
        p,
        &format!("menu-btn-{id}"),
        &icons::MORE_VERT,
        30.0,
        15.0,
        "Clip menu",
        false,
        move |st| st.open_context_menu(id, 480.0, 260.0),
    ));

    row()
        .key(format!("lib-row-{id}"))
        .w_full()
        .padding(space::SM)
        .gap(space::MD)
        .justify_between()
        .center_y()
        .cursor(Cursor::Pointer)
        .fill(if is_selected {
            p.bg_elevated
        } else {
            p.bg_surface
        })
        .border(
            1.0,
            if is_selected {
                p.border_focus
            } else {
                p.border_subtle
            },
        )
        .hover(p.bg_hover)
        .motion(snappy_spring())
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
                .grow(1.0)
                .min_w(0.0)
                .child(checkbox(
                    p,
                    &format!("select-check-{id}"),
                    "Select clip",
                    is_multi,
                    move |st| st.toggle_clip_in_selection(id),
                ))
                .child(thumb)
                .child(
                    col()
                        .key("info")
                        .gap(3.0)
                        .grow(1.0)
                        .min_w(0.0)
                        .child(
                            row()
                                .key("list-title-row")
                                .gap(space::SM)
                                .center_y()
                                .child(
                                    text(clip.title.clone())
                                        .font(typography::BODY)
                                        .color(p.text_primary),
                                )
                                .child(origin_badge(p, "origin", origin)),
                        )
                        .child(
                            text(format!(
                                "{game} • {}x{} @ {}fps • {} • {} • {}",
                                clip.width,
                                clip.height,
                                clip.fps as u32,
                                format_duration_ms(clip.duration_ms),
                                format_bytes(clip.size_bytes),
                                clip.created_at.format("%Y-%m-%d %H:%M")
                            ))
                            .font(typography::LABEL)
                            .color(p.text_muted),
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
    let res_codec = format!("{}p {}", clip.height, clip.codec.to_uppercase());
    let overlay_bg = Color {
        a: 0.82,
        ..p.bg_lowest
    };

    let thumb_visual = if let Some(px) = s.library_thumbs.get(&id) {
        el().key("thumb-img")
            .w_full()
            .h_full()
            .pixels_fit(px, SurfaceFit::Fit)
    } else {
        el().key("thumb-empty").w_full().h_full().center().child(
            text(game.to_string())
                .font(typography::MICRO)
                .color(p.text_muted),
        )
    };

    let thumb_overlay = col()
        .key("thumb-overlay")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .padding(space::SM)
        .justify_between()
        .child(
            row()
                .key("thumb-top")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    row()
                        .key("thumb-top-left")
                        .gap(space::XS)
                        .center_y()
                        .child(checkbox(
                            p,
                            &format!("select-check-{id}"),
                            "Select clip",
                            is_multi,
                            move |st| st.toggle_clip_in_selection(id),
                        ))
                        .child(badge("game-pill", game, p.text_primary, overlay_bg)),
                )
                .child(
                    row()
                        .key("thumb-top-right")
                        .gap(space::XS)
                        .center_y()
                        .child(badge("res-pill", &res_codec, p.text_secondary, overlay_bg))
                        .child(pill_option(
                            p,
                            &format!("fav-{id}"),
                            if fav { "★" } else { "☆" },
                            fav,
                            move |st| st.toggle_favorite(id),
                        )),
                ),
        )
        .child(
            row()
                .key("thumb-bottom")
                .w_full()
                .justify_between()
                .center_y()
                .child(origin_badge(p, "origin", origin))
                .child(badge(
                    "dur",
                    &format_duration_ms(clip.duration_ms),
                    p.text_primary,
                    overlay_bg,
                )),
        );

    let thumb_box = el()
        .key("thumb")
        .w_full()
        .h(168.0)
        .fill(p.bg_lowest)
        .border_sides([0.0, 0.0, 1.0, 0.0], p.border_subtle)
        .child(thumb_visual)
        .child(thumb_overlay);

    let mut card_actions = row().key("card-actions").gap(space::XS).center_y();
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
            .child(btn_primary(
                p,
                &format!("edit-{id}"),
                "Trim & Edit",
                move |st| st.open_in_editor(id),
            ))
            .child(btn_secondary(p, &format!("play-{id}"), "Play", move |st| {
                st.play_in_editor(id)
            }))
            .child(btn_secondary(
                p,
                &format!("trash-{id}"),
                "Trash",
                move |st| st.request_move_to_trash(vec![id]),
            ));
    }

    let footer = col()
        .key("card-footer")
        .w_full()
        .padding(space::MD)
        .gap(space::SM)
        .child(
            row()
                .key("title-row")
                .w_full()
                .justify_between()
                .center_y()
                .gap(space::XS)
                .child(
                    text(clip.title.clone())
                        .font(typography::HEADLINE_SM)
                        .color(p.text_primary)
                        .grow(1.0)
                        .min_w(0.0),
                )
                .child(icon_btn_sized(
                    p,
                    &format!("menu-btn-{id}"),
                    &icons::MORE_VERT,
                    28.0,
                    14.0,
                    "Clip menu",
                    false,
                    move |st| st.open_context_menu(id, 480.0, 260.0),
                )),
        )
        .child(
            row()
                .key("meta-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    text(format!(
                        "{game} • {}",
                        clip.created_at.format("%Y-%m-%d %H:%M")
                    ))
                    .font(typography::LABEL)
                    .color(p.text_muted),
                )
                .child(
                    text(format_bytes(clip.size_bytes))
                        .font(typography::MONO_BADGE)
                        .color(p.text_secondary),
                ),
        )
        .child(card_actions);

    clip_card_frame(p, &format!("lib-card-{id}"), is_selected)
        .grow(1.0)
        .min_w(0.0)
        .cursor(Cursor::Pointer)
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
        .child(footer)
}
