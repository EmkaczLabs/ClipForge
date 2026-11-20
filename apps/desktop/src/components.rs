//! Reusable Rune UI components for ClipForge Desktop.

use rune::tok::type_;
use rune::{Color, Cursor, El, col, el, row, text};

use crate::state::AppState;
use crate::theme::{Palette, radius, snappy_spring, space};

/// Surface card container with subtle 1px border and rounded corners.
pub fn card(p: &Palette, key: &str) -> El {
    col()
        .key(key)
        .padding(space::LG)
        .gap(space::MD)
        .fill(p.bg_surface)
        .radius(radius::LG)
        .border(1.0, p.border_subtle)
}

/// Compact pill badge for tags, codecs, and statuses.
pub fn badge(key: &str, label_text: &str, fg: Color, bg: Color) -> El {
    row()
        .key(key)
        .px(space::SM)
        .py(3.0)
        .radius(radius::PILL)
        .fill(bg)
        .center_y()
        .child(
            text(label_text.to_string())
                .font(type_::LABEL)
                .color(fg)
                .a11y_hidden(),
        )
}

/// Primary accent button.
pub fn btn_primary(
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
        .px(space::MD)
        .h(34.0)
        .center()
        .radius(radius::MD)
        .fill(p.accent)
        .hover(p.bg_active)
        .press(p.bg_elevated)
        .motion(snappy_spring())
        .on_press(on_press)
        .child(
            text(label_text.to_string())
                .font(type_::LABEL)
                .color(p.accent_text)
                .a11y_hidden(),
        )
}

/// Secondary subtle button.
pub fn btn_secondary(
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
        .px(space::MD)
        .h(34.0)
        .center()
        .radius(radius::MD)
        .fill(p.bg_elevated)
        .border(1.0, p.border_subtle)
        .hover(p.bg_hover)
        .press(p.bg_active)
        .motion(snappy_spring())
        .on_press(on_press)
        .child(
            text(label_text.to_string())
                .font(type_::LABEL)
                .color(p.text_primary)
                .a11y_hidden(),
        )
}

/// Danger / recording action button.
pub fn btn_danger(
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
        .px(space::MD)
        .h(34.0)
        .center()
        .radius(radius::MD)
        .fill(p.recording_soft)
        .border(1.0, p.recording)
        .hover(p.recording)
        .motion(snappy_spring())
        .on_press(on_press)
        .child(
            text(label_text.to_string())
                .font(type_::LABEL)
                .color(p.text_primary)
                .a11y_hidden(),
        )
}

/// Compact pill selector button (used for filters and segmented options).
pub fn pill_option(
    p: &Palette,
    key: &str,
    label_text: &str,
    selected: bool,
    on_press: impl Fn(&mut AppState) + 'static,
) -> El {
    row()
        .key(key)
        .role("button")
        .label(label_text)
        .checked(selected)
        .focusable(true)
        .cursor(Cursor::Pointer)
        .px(space::MD)
        .h(30.0)
        .center()
        .radius(radius::SM)
        .fill(if selected {
            p.accent_soft
        } else {
            p.bg_elevated
        })
        .border(1.0, if selected { p.accent } else { p.border_subtle })
        .hover(p.bg_hover)
        .motion(snappy_spring())
        .on_press(on_press)
        .child(
            text(label_text.to_string())
                .font(type_::LABEL)
                .color(if selected {
                    p.text_primary
                } else {
                    p.text_secondary
                })
                .a11y_hidden(),
        )
}

/// Animated switch control.
pub fn toggle_switch(
    p: &Palette,
    key: &str,
    label_text: &str,
    on: bool,
    on_press: impl Fn(&mut AppState) + 'static,
) -> El {
    let (w, h, k) = (38.0, 22.0, 18.0);
    let inset = (h - k) / 2.0;
    el().key(key)
        .role("switch")
        .label(label_text)
        .checked(on)
        .focusable(true)
        .cursor(Cursor::Pointer)
        .size(w, h)
        .radius(h / 2.0)
        .fill(if on { p.accent } else { p.control_off })
        .motion(snappy_spring())
        .on_press(on_press)
        .child(
            el().key("knob")
                .size(k, k)
                .absolute(if on { w - k - inset } else { inset }, inset)
                .radius(k / 2.0)
                .fill(p.knob)
                .a11y_hidden(),
        )
}

/// Dashboard KPI tile.
pub fn stat_tile(p: &Palette, key: &str, label_text: &str, value: String, hint: &str) -> El {
    card(p, key)
        .grow(1.0)
        .gap(space::XS)
        .child(
            text(label_text.to_string())
                .font(type_::LABEL)
                .color(p.text_secondary),
        )
        .child(text(value).font(type_::TITLE).color(p.text_primary))
        .child(
            text(hint.to_string())
                .font(type_::LABEL)
                .color(p.text_muted),
        )
}

/// Section header with title and muted subtitle.
pub fn section_header(p: &Palette, key: &str, title: &str, subtitle: &str) -> El {
    col()
        .key(key)
        .gap(2.0)
        .child(
            text(title.to_string())
                .font(type_::SECTION)
                .color(p.text_primary),
        )
        .child(
            text(subtitle.to_string())
                .font(type_::LABEL)
                .color(p.text_secondary),
        )
}

/// Key-value metadata row.
pub fn kv_row(p: &Palette, key: &str, label_text: &str, value: String) -> El {
    row()
        .key(key)
        .w_full()
        .py(space::XS)
        .justify_between()
        .center_y()
        .child(
            text(label_text.to_string())
                .font(type_::BODY)
                .color(p.text_secondary),
        )
        .child(text(value).font(type_::BODY).color(p.text_primary))
}
