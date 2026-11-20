//! Application shell and 6-section views for ClipForge Desktop.

pub mod dashboard;
pub mod editor;
pub mod library;
pub mod recording;
pub mod settings;
pub mod uploads;

use clipforge_common::CaptureStatus;
use rune::tok::type_;
use rune::{Cursor, El, ScrollSpec, col, el, row, text};

use crate::components::{badge, btn_danger, btn_primary, btn_secondary};
use crate::state::{AppState, NavSection};
use crate::theme::{Palette, radius, snappy_spring, space};

/// Root Rune view function rendering the sidebar shell, topbar, and active section.
pub fn root_view(s: &AppState, _cx: &mut rune::Cx<AppState>) -> El {
    let p = Palette::dark();

    let shell = row()
        .key("clipforge-shell")
        .role("window")
        .w_full()
        .h_full()
        .fill(p.bg_base)
        .child(sidebar(&p, s))
        .child(
            col()
                .key("main-pane")
                .grow(1.0)
                .h_full()
                .min_h(0.0)
                .child(topbar(&p, s))
                .child(banner_strip(&p, s))
                .child(
                    col()
                        .key("content-scroll")
                        .grow(1.0)
                        .min_h(0.0)
                        .padding(space::XL)
                        .gap(space::LG)
                        .scroll(ScrollSpec {
                            stick_bottom: false,
                        })
                        .child(active_view(&p, s)),
                ),
        );

    el().key("clipforge-root")
        .w_full()
        .h_full()
        .child(shell)
        .children(context_menu_overlay(&p, s))
}

fn context_menu_overlay(p: &Palette, s: &AppState) -> Option<El> {
    let menu = s.context_menu.as_ref()?;
    let id = menu.clip_id;
    let is_fav = s.favorite_ids.contains(&id);
    let x = menu.x.clamp(16.0, 980.0);
    let y = menu.y.clamp(16.0, 480.0);

    let mut items = col()
        .key("context-menu")
        .absolute(x, y)
        .w(210.0)
        .padding(space::SM)
        .gap(space::XS)
        .fill(p.bg_elevated)
        .border(1.0, p.accent)
        .radius(radius::MD);

    if menu.is_trashed {
        items = items
            .child(ctx_item(
                p,
                "ctx-restore",
                "Restore from Trash",
                move |st| {
                    st.request_restore_from_trash(vec![id]);
                },
            ))
            .child(ctx_item(
                p,
                "ctx-open-folder",
                "Open Containing Folder",
                move |st| st.open_containing_folder(id),
            ))
            .child(ctx_danger_item(
                p,
                "ctx-perm-delete",
                "Delete Permanently",
                move |st| st.request_permanent_delete(vec![id]),
            ));
    } else {
        items = items
            .child(ctx_item(p, "ctx-edit", "Open in Editor", move |st| {
                st.close_context_menu();
                st.open_in_editor(id);
            }))
            .child(ctx_item(p, "ctx-player", "Open in Player", move |st| {
                st.close_context_menu();
                st.open_clip_in_player(id);
            }))
            .child(ctx_item(
                p,
                "ctx-fav",
                if is_fav {
                    "Remove Favorite"
                } else {
                    "Add to Favorites"
                },
                move |st| {
                    st.close_context_menu();
                    st.toggle_favorite(id);
                },
            ))
            .child(ctx_item(p, "ctx-rename", "Rename Clip", move |st| {
                st.begin_rename(id);
            }))
            .child(ctx_item(
                p,
                "ctx-open-folder",
                "Open Containing Folder",
                move |st| st.open_containing_folder(id),
            ))
            .child(ctx_item(p, "ctx-upload", "Upload / Share", move |st| {
                st.close_context_menu();
                st.queue_upload(id);
            }))
            .child(ctx_item(p, "ctx-trash", "Move to Trash", move |st| {
                st.request_move_to_trash(vec![id]);
            }))
            .child(ctx_danger_item(
                p,
                "ctx-perm-delete",
                "Delete Permanently",
                move |st| st.request_permanent_delete(vec![id]),
            ));
    }

    Some(
        el().key("context-menu-layer")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .child(
                el().key("context-menu-scrim")
                    .w_full()
                    .h_full()
                    .on_background_press(|st: &mut AppState| st.close_context_menu()),
            )
            .child(items),
    )
}

fn ctx_item(
    p: &Palette,
    key: &str,
    label_text: &str,
    on_press: impl Fn(&mut AppState) + 'static,
) -> El {
    row()
        .key(key)
        .role("button")
        .label(label_text)
        .focusable(true)
        .cursor(Cursor::Pointer)
        .w_full()
        .h(30.0)
        .px(space::SM)
        .center_y()
        .radius(radius::SM)
        .fill(rune::Color::TRANSPARENT)
        .hover(p.bg_hover)
        .on_press(on_press)
        .child(
            text(label_text.to_string())
                .font(type_::LABEL)
                .color(p.text_primary)
                .a11y_hidden(),
        )
}

fn ctx_danger_item(
    p: &Palette,
    key: &str,
    label_text: &str,
    on_press: impl Fn(&mut AppState) + 'static,
) -> El {
    row()
        .key(key)
        .role("button")
        .label(label_text)
        .focusable(true)
        .cursor(Cursor::Pointer)
        .w_full()
        .h(30.0)
        .px(space::SM)
        .center_y()
        .radius(radius::SM)
        .fill(p.recording_soft)
        .hover(p.recording)
        .on_press(on_press)
        .child(
            text(label_text.to_string())
                .font(type_::LABEL)
                .color(p.text_primary)
                .a11y_hidden(),
        )
}

fn sidebar(p: &Palette, s: &AppState) -> El {
    let mut nav_list = col().key("nav-list").gap(space::XS);
    for section in NavSection::ALL {
        let selected = s.nav == section;
        nav_list = nav_list.child(
            row()
                .key(section.key())
                .role("button")
                .label(section.label())
                .checked(selected)
                .focusable(true)
                .cursor(Cursor::Pointer)
                .w_full()
                .h(36.0)
                .px(space::MD)
                .center_y()
                .gap(space::SM)
                .radius(radius::MD)
                .fill(if selected {
                    p.bg_active
                } else {
                    rune::Color::TRANSPARENT
                })
                .hover(p.bg_hover)
                .motion(snappy_spring())
                .on_press(move |st: &mut AppState| st.nav = section)
                .child(
                    el().key("indicator")
                        .size(6.0, 6.0)
                        .radius(3.0)
                        .fill(if selected { p.accent } else { p.text_muted })
                        .a11y_hidden(),
                )
                .child(
                    text(section.label().to_string())
                        .font(type_::BODY)
                        .color(if selected {
                            p.text_primary
                        } else {
                            p.text_secondary
                        })
                        .a11y_hidden(),
                ),
        );
    }

    let (status_label, dot_color, pill_bg) = capture_pill(p, &s.capture_status);

    col()
        .key("sidebar")
        .w(224.0)
        .h_full()
        .padding(space::LG)
        .gap(space::LG)
        .fill(p.bg_sidebar)
        .border(1.0, p.border_subtle)
        .child(
            row()
                .key("brand")
                .center_y()
                .justify_between()
                .child(text("ClipForge").font(type_::TITLE).color(p.text_primary))
                .child(badge("v-badge", "v0.1", p.text_secondary, p.bg_elevated)),
        )
        .child(
            row()
                .key("status-pill")
                .px(space::MD)
                .py(space::SM)
                .gap(space::SM)
                .center_y()
                .radius(radius::MD)
                .fill(pill_bg)
                .border(1.0, p.border_subtle)
                .child(
                    el().key("status-dot")
                        .size(8.0, 8.0)
                        .radius(4.0)
                        .fill(dot_color)
                        .a11y_hidden(),
                )
                .child(text(status_label).font(type_::LABEL).color(p.text_primary)),
        )
        .child(nav_list)
        .child(el().key("sidebar-spacer").grow(1.0))
        .child(
            col()
                .key("sidebar-footer")
                .gap(space::SM)
                .child(btn_primary(
                    p,
                    "quick-save-replay",
                    "Save Replay (F9)",
                    |st| {
                        st.trigger_save_replay();
                    },
                ))
                .child(
                    text(format!("{} clips in library", s.clips.len()))
                        .font(type_::LABEL)
                        .color(p.text_muted),
                ),
        )
}

fn topbar(p: &Palette, s: &AppState) -> El {
    let is_buffering = matches!(s.capture_status, CaptureStatus::ReplayBuffering { .. });
    row()
        .key("topbar")
        .w_full()
        .h(56.0)
        .px(space::XL)
        .center_y()
        .justify_between()
        .border(1.0, p.border_subtle)
        .child(
            text(s.nav.label().to_string())
                .key("page-title")
                .font(type_::TITLE)
                .color(p.text_primary),
        )
        .child(
            row()
                .key("topbar-actions")
                .gap(space::SM)
                .center_y()
                .child(if is_buffering {
                    btn_danger(p, "topbar-replay-toggle", "Stop Replay", |st| {
                        st.toggle_replay_buffer();
                    })
                } else {
                    btn_secondary(p, "topbar-replay-toggle", "Start Replay", |st| {
                        st.toggle_replay_buffer();
                    })
                })
                .child(btn_primary(p, "topbar-save-clip", "Clip Now", |st| {
                    st.trigger_save_replay();
                })),
        )
}

fn banner_strip(p: &Palette, s: &AppState) -> El {
    let Some(msg) = &s.status_banner else {
        return el().key("banner-empty").h(0.0);
    };
    row()
        .key("status-banner")
        .w_full()
        .px(space::XL)
        .py(space::SM)
        .justify_between()
        .center_y()
        .fill(p.accent_soft)
        .border(1.0, p.accent)
        .child(text(msg.clone()).font(type_::LABEL).color(p.text_primary))
        .child(btn_secondary(p, "dismiss-banner", "Dismiss", |st| {
            st.status_banner = None;
        }))
}

fn active_view(p: &Palette, s: &AppState) -> El {
    match s.nav {
        NavSection::Dashboard => dashboard::view(p, s),
        NavSection::Library => library::view(p, s),
        NavSection::Editor => editor::view(p, s),
        NavSection::Recording => recording::view(p, s),
        NavSection::Uploads => uploads::view(p, s),
        NavSection::Settings => settings::view(p, s),
    }
}

fn capture_pill(p: &Palette, status: &CaptureStatus) -> (String, rune::Color, rune::Color) {
    match status {
        CaptureStatus::Idle => ("Idle".to_string(), p.text_muted, p.bg_surface),
        CaptureStatus::ReplayBuffering { buffer_secs, .. } => (
            format!("Replay ({buffer_secs}s)"),
            p.success,
            p.success_soft,
        ),
        CaptureStatus::Recording { paused, .. } => {
            let label = if *paused {
                "Recording (Paused)".to_string()
            } else {
                "Recording Live".to_string()
            };
            (label, p.recording, p.recording_soft)
        }
        CaptureStatus::Error { .. } => ("Capture Error".to_string(), p.warning, p.warning_soft),
    }
}
