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

    row()
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
