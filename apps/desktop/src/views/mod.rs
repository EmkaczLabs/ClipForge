//! Shared Obsidian Forge application shell and view router for ClipForge Desktop.

pub mod dashboard;
pub mod editor;
pub mod library;
pub mod recording;
pub mod settings;
pub mod uploads;

use clipforge_common::{CaptureStatus, CaptureTarget};
use rune::{Color, Cursor, El, Icon, ScrollSpec, col, el, icon, row, text};

use crate::components::{
    badge, btn_danger, btn_primary, btn_secondary, context_menu_shell, icon_btn_sized, icons,
    status_pill,
};
use crate::state::{AppState, NavSection};
use crate::theme::{Palette, radius, snappy_spring, space, typography};

/// Root Rune view function rendering the topbar, 64px icon rail, workspace, and docked status bar.
pub fn root_view(s: &AppState, _cx: &mut rune::Cx<AppState>) -> El {
    let p = Palette::dark();

    let content_area = if s.nav == NavSection::Editor {
        col()
            .key("content-stage")
            .grow(1.0)
            .min_h(0.0)
            .child(active_view(&p, s))
    } else {
        col()
            .key("content-scroll")
            .grow(1.0)
            .min_h(0.0)
            .padding(space::LG)
            .gap(space::LG)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(active_view(&p, s))
    };

    let shell = col()
        .key("clipforge-shell")
        .role("window")
        .w_full()
        .h_full()
        .fill(p.bg_base)
        .child(topbar(&p, s))
        .child(
            row()
                .key("workspace-row")
                .grow(1.0)
                .min_h(0.0)
                .w_full()
                .child(icon_rail(&p, s))
                .child(
                    col()
                        .key("main-pane")
                        .grow(1.0)
                        .min_w(0.0)
                        .h_full()
                        .min_h(0.0)
                        .fill(p.bg_base)
                        .child(banner_strip(&p, s))
                        .child(content_area),
                ),
        )
        .child(bottom_status_bar(&p, s));

    el().key("clipforge-root")
        .w_full()
        .h_full()
        .child(shell)
        .children(context_menu_overlay(&p, s))
}

fn topbar(p: &Palette, s: &AppState) -> El {
    let is_buffering = matches!(s.capture_status, CaptureStatus::ReplayBuffering { .. });
    let (pill_label, dot_color, fg, pill_bg, hk) = capture_pill_spec(p, s);
    let target_summary = format!(
        "{} • {} audio",
        target_label(&s.config.capture.capture_target),
        s.config.capture.audio_sources.len()
    );

    row()
        .key("topbar")
        .w_full()
        .h(48.0)
        .px(space::LG)
        .center_y()
        .justify_between()
        .fill(p.bg_lowest)
        .border_sides([0.0, 0.0, 1.0, 0.0], p.border_subtle)
        .child(
            row()
                .key("topbar-brand")
                .gap(space::SM)
                .center_y()
                .child(
                    row()
                        .key("brand-mark")
                        .size(28.0, 28.0)
                        .center()
                        .radius(radius::SM)
                        .fill(p.bg_surface)
                        .border(1.0, p.border_subtle)
                        .child(
                            icon(&icons::BRAND_FORGE, 16.0)
                                .color(p.accent_primary)
                                .a11y_hidden(),
                        ),
                )
                .child(
                    text("ClipForge")
                        .font(typography::HEADLINE_SM)
                        .color(p.text_primary),
                )
                .child(badge(
                    "v-badge",
                    concat!("v", env!("CARGO_PKG_VERSION")),
                    p.text_muted,
                    p.bg_surface,
                ))
                .child(text("•").font(typography::LABEL).color(p.text_disabled))
                .child(
                    text(s.nav.label().to_string())
                        .key("page-title")
                        .font(typography::LABEL)
                        .color(p.text_secondary),
                ),
        )
        .child(
            row()
                .key("topbar-telemetry")
                .gap(space::SM)
                .center_y()
                .child(status_pill(
                    p,
                    "status-pill",
                    dot_color,
                    &pill_label,
                    fg,
                    pill_bg,
                    hk,
                ))
                .child(badge(
                    "topbar-target-pill",
                    &target_summary,
                    p.text_secondary,
                    p.bg_surface,
                )),
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
                .child(btn_primary(p, "topbar-save-clip", "Save Replay", |st| {
                    st.trigger_save_replay();
                })),
        )
}

fn icon_rail(p: &Palette, s: &AppState) -> El {
    let mut nav_list = col().key("nav-list").w_full().gap(space::SM);
    for section in NavSection::ALL {
        if matches!(section, NavSection::Dashboard) {
            continue;
        }
        nav_list = nav_list.child(rail_nav_item(p, s, section));
    }

    let gsr_dot = if s.deps.gsr_available {
        p.success
    } else {
        p.warning
    };
    let target_tag = target_short(&s.config.capture.capture_target);

    col()
        .key("sidebar")
        .w(64.0)
        .h_full()
        .py(space::MD)
        .gap(space::MD)
        .fill(p.bg_lowest)
        .border_sides([0.0, 1.0, 0.0, 0.0], p.border_subtle)
        .child(nav_list)
        .child(el().key("sidebar-spacer").grow(1.0))
        .child(
            row().key("rail-footer-wrap").w_full().center().child(
                col()
                    .key("rail-footer")
                    .w(44.0)
                    .py(space::SM)
                    .gap(2.0)
                    .center()
                    .radius(radius::MD)
                    .fill(p.bg_panel)
                    .border(1.0, p.border_subtle)
                    .child(
                        el().key("rail-gsr-dot")
                            .size(7.0, 7.0)
                            .radius(3.5)
                            .fill(gsr_dot)
                            .a11y_hidden(),
                    )
                    .child(
                        text(target_tag.to_string())
                            .font(typography::MONO_BADGE)
                            .color(if s.deps.gsr_available {
                                p.success_text
                            } else {
                                p.warning_text
                            }),
                    ),
            ),
        )
}

fn rail_nav_item(p: &Palette, s: &AppState, section: NavSection) -> El {
    let selected = s.nav == section;
    let ic = nav_icon(section);

    let indicator = el()
        .key("active-bar")
        .w(3.0)
        .h(if selected { 24.0 } else { 0.0 })
        .radius(2.0)
        .fill(p.accent_primary)
        .motion(snappy_spring())
        .a11y_hidden();

    let btn = icon_btn_sized(
        p,
        section.key(),
        ic,
        40.0,
        20.0,
        section.label(),
        selected,
        move |st: &mut AppState| st.nav = section,
    );

    let overlay_badge: Option<El> = match section {
        NavSection::Library if !s.clips.is_empty() => Some(
            row()
                .key("rail-lib-count")
                .absolute(36.0, -2.0)
                .px(4.0)
                .py(1.0)
                .radius(radius::PILL)
                .fill(p.accent_primary)
                .a11y_hidden()
                .child(
                    text(s.clips.len().to_string())
                        .font(typography::MONO_BADGE)
                        .color(p.on_accent_primary),
                ),
        ),
        NavSection::Recording => match s.capture_status {
            CaptureStatus::Recording { .. } => Some(rail_status_dot("rail-rec-dot", p.recording)),
            CaptureStatus::ReplayBuffering { .. } => {
                Some(rail_status_dot("rail-rec-dot", p.success))
            }
            _ => None,
        },
        NavSection::Uploads if s.config.server_url.is_some() => {
            Some(rail_status_dot("rail-cloud-dot", p.success))
        }
        _ => None,
    };

    row()
        .key(format!("rail-wrap-{}", section.key()))
        .w_full()
        .center_y()
        .child(indicator)
        .child(
            el().key("rail-btn-wrap")
                .grow(1.0)
                .center()
                .child(btn)
                .children(overlay_badge),
        )
}

fn rail_status_dot(key: &str, color: Color) -> El {
    el().key(key)
        .size(8.0, 8.0)
        .radius(4.0)
        .absolute(42.0, 2.0)
        .fill(color)
        .a11y_hidden()
}

fn bottom_status_bar(p: &Palette, s: &AppState) -> El {
    let (status_text, dot_color, text_color) = match &s.capture_status {
        CaptureStatus::ReplayBuffering { buffer_secs, .. } => (
            format!("REPLAY BUFFER ACTIVE ({buffer_secs}s)"),
            p.success,
            p.success_text,
        ),
        CaptureStatus::Recording { paused, .. } => (
            if *paused {
                "RECORDING PAUSED".to_string()
            } else {
                "RECORDING LIVE".to_string()
            },
            p.recording,
            p.error_text,
        ),
        CaptureStatus::Idle => ("CAPTURE IDLE".to_string(), p.text_muted, p.text_muted),
        CaptureStatus::Error { .. } => ("CAPTURE ERROR".to_string(), p.warning, p.warning_text),
    };

    let pipeline = format!(
        "{:?} • {} FPS • {}",
        s.config.capture.encoder,
        s.config.capture.fps,
        target_label(&s.config.capture.capture_target)
    );
    let deps_text = format!(
        "GSR: {} • FFmpeg: {}",
        if s.deps.gsr_available {
            "OK"
        } else {
            "Missing"
        },
        if s.deps.ffmpeg_available {
            "OK"
        } else {
            "Missing"
        }
    );

    row()
        .key("bottom-status-bar")
        .w_full()
        .h(28.0)
        .px(space::LG)
        .center_y()
        .justify_between()
        .fill(p.bg_lowest)
        .border_sides([1.0, 0.0, 0.0, 0.0], p.border_subtle)
        .child(
            row()
                .key("status-left")
                .gap(space::SM)
                .center_y()
                .child(
                    el().key("bottom-status-dot")
                        .size(7.0, 7.0)
                        .radius(3.5)
                        .fill(dot_color)
                        .a11y_hidden(),
                )
                .child(
                    text(status_text)
                        .font(typography::MONO_BADGE)
                        .color(text_color),
                )
                .child(
                    text("•")
                        .font(typography::MONO_BADGE)
                        .color(p.text_disabled),
                )
                .child(
                    text(pipeline)
                        .font(typography::MONO_BADGE)
                        .color(p.text_muted),
                ),
        )
        .child(
            row()
                .key("status-right")
                .gap(space::MD)
                .center_y()
                .child(
                    text(format!("Hit {} to save replay", s.hotkeys.save_replay))
                        .font(typography::MONO_BADGE)
                        .color(p.text_muted),
                )
                .child(
                    text("•")
                        .font(typography::MONO_BADGE)
                        .color(p.text_disabled),
                )
                .child(text(deps_text).font(typography::MONO_BADGE).color(
                    if s.deps.gsr_available && s.deps.ffmpeg_available {
                        p.text_secondary
                    } else {
                        p.warning_text
                    },
                )),
        )
}

fn banner_strip(p: &Palette, s: &AppState) -> El {
    let Some(msg) = &s.status_banner else {
        return el().key("banner-empty").h(0.0);
    };
    row()
        .key("status-banner")
        .w_full()
        .px(space::LG)
        .py(space::SM)
        .justify_between()
        .center_y()
        .fill(p.accent_soft)
        .border_sides([0.0, 0.0, 1.0, 0.0], p.accent_brand)
        .child(
            text(msg.clone())
                .font(typography::LABEL)
                .color(p.text_primary),
        )
        .child(btn_secondary(p, "dismiss-banner", "Dismiss", |st| {
            st.status_banner = None;
        }))
}

fn context_menu_overlay(p: &Palette, s: &AppState) -> Option<El> {
    let menu = s.context_menu.as_ref()?;
    let id = menu.clip_id;
    let is_fav = s.favorite_ids.contains(&id);
    let x = menu.x.clamp(16.0, 980.0);
    let y = menu.y.clamp(16.0, 480.0);

    let items = if menu.is_trashed {
        vec![
            ctx_item(p, "ctx-restore", "Restore from Trash", move |st| {
                st.request_restore_from_trash(vec![id]);
            }),
            ctx_item(p, "ctx-open-folder", "Open Containing Folder", move |st| {
                st.open_containing_folder(id);
            }),
            ctx_danger_item(p, "ctx-perm-delete", "Delete Permanently", move |st| {
                st.request_permanent_delete(vec![id]);
            }),
        ]
    } else {
        vec![
            ctx_item(p, "ctx-edit", "Open in Editor", move |st| {
                st.close_context_menu();
                st.open_in_editor(id);
            }),
            ctx_item(p, "ctx-player", "Open in Player", move |st| {
                st.close_context_menu();
                st.open_clip_in_player(id);
            }),
            ctx_item(
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
            ),
            ctx_item(p, "ctx-rename", "Rename Clip", move |st| {
                st.begin_rename(id);
            }),
            ctx_item(p, "ctx-open-folder", "Open Containing Folder", move |st| {
                st.open_containing_folder(id);
            }),
            ctx_item(p, "ctx-upload", "Upload / Share", move |st| {
                st.close_context_menu();
                st.queue_upload(id);
            }),
            ctx_item(p, "ctx-trash", "Move to Trash", move |st| {
                st.request_move_to_trash(vec![id]);
            }),
            ctx_danger_item(p, "ctx-perm-delete", "Delete Permanently", move |st| {
                st.request_permanent_delete(vec![id]);
            }),
        ]
    };

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
            .child(context_menu_shell(p, "context-menu", x, y, items)),
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
        .fill(Color::TRANSPARENT)
        .hover(p.bg_hover)
        .on_press(on_press)
        .child(
            text(label_text.to_string())
                .font(typography::LABEL)
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
                .font(typography::LABEL)
                .color(p.error_text)
                .a11y_hidden(),
        )
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

fn nav_icon(section: NavSection) -> &'static Icon {
    match section {
        NavSection::Dashboard | NavSection::Library => &icons::LIBRARY,
        NavSection::Editor => &icons::EDITOR,
        NavSection::Recording => &icons::RECORDING,
        NavSection::Uploads => &icons::CLOUD,
        NavSection::Settings => &icons::SETTINGS,
    }
}

fn target_label(target: &CaptureTarget) -> &'static str {
    match target {
        CaptureTarget::PrimaryScreen => "Primary Screen",
        CaptureTarget::Monitor(_) => "Monitor",
        CaptureTarget::FocusedWindow => "Focused Window",
        CaptureTarget::Portal => "Portal Select",
        CaptureTarget::Region { .. } => "Region",
    }
}

fn target_short(target: &CaptureTarget) -> &'static str {
    match target {
        CaptureTarget::PrimaryScreen => "SCR",
        CaptureTarget::Monitor(_) => "MON",
        CaptureTarget::FocusedWindow => "WIN",
        CaptureTarget::Portal => "PRT",
        CaptureTarget::Region { .. } => "REG",
    }
}

fn capture_pill_spec<'a>(
    p: &Palette,
    s: &'a AppState,
) -> (String, Color, Color, Color, Option<&'a str>) {
    match &s.capture_status {
        CaptureStatus::Idle => (
            "IDLE".to_string(),
            p.text_muted,
            p.text_muted,
            p.bg_surface,
            None,
        ),
        CaptureStatus::ReplayBuffering { buffer_secs, .. } => (
            format!("BUFFER {buffer_secs}s"),
            p.success,
            p.success_text,
            p.success_soft,
            Some(s.hotkeys.save_replay.as_str()),
        ),
        CaptureStatus::Recording { paused, .. } => {
            let label = if *paused {
                "REC (PAUSED)".to_string()
            } else {
                "REC".to_string()
            };
            (
                label,
                p.recording,
                p.error_text,
                p.recording_soft,
                Some(s.hotkeys.toggle_record.as_str()),
            )
        }
        CaptureStatus::Error { .. } => (
            "CAPTURE ERROR".to_string(),
            p.warning,
            p.warning_text,
            p.warning_soft,
            None,
        ),
    }
}
