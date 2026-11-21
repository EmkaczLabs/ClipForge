//! Reusable Obsidian Forge Rune UI components for ClipForge Desktop.

use rune::{Color, Cursor, El, Icon, TextEdit, col, el, icon, row, text, text_input};

use crate::state::AppState;
use crate::theme::{Palette, radius, snappy_spring, space, typography};

/// Minimal crisp 24x24 SVG icon catalog for ClipForge Desktop.
#[allow(dead_code)]
pub mod icons {
    use rune::Icon;

    pub static BRAND_FORGE: Icon = Icon {
        name: "brand_forge",
        view: 24.0,
        body: r#"<path d="M5 6h14l-2 5H7L5 6z" fill="currentColor"/><path d="M4 13h16v2.5c0 1.4-1.1 2.5-2.5 2.5h-11C5.1 18 4 16.9 4 15.5V13z" fill="currentColor" opacity="0.85"/><path d="M9 18h6v2H9v-2z" fill="currentColor"/>"#,
    };

    pub static LIBRARY: Icon = Icon {
        name: "library",
        view: 24.0,
        body: r#"<rect x="3" y="4" width="8" height="7" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.8"/><rect x="13" y="4" width="8" height="7" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.8"/><rect x="3" y="13" width="8" height="7" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.8"/><rect x="13" y="13" width="8" height="7" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.8"/>"#,
    };

    pub static EDITOR: Icon = Icon {
        name: "editor",
        view: 24.0,
        body: r#"<rect x="3" y="5" width="18" height="14" rx="2" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="M9 5v14M15 5v14M3 12h18" stroke="currentColor" stroke-width="1.6"/>"#,
    };

    pub static RECORDING: Icon = Icon {
        name: "recording",
        view: 24.0,
        body: r#"<circle cx="12" cy="12" r="8" fill="none" stroke="currentColor" stroke-width="1.8"/><circle cx="12" cy="12" r="4" fill="currentColor"/>"#,
    };

    pub static CLOUD: Icon = Icon {
        name: "cloud",
        view: 24.0,
        body: r#"<path d="M7.5 18h9.5a4 4 0 0 0 .6-7.95A5.5 5.5 0 0 0 7 9.5a4.2 4.2 0 0 0 .5 8.5z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/>"#,
    };

    pub static SETTINGS: Icon = Icon {
        name: "settings",
        view: 24.0,
        body: r#"<path d="M4 7h10M18 7h2M4 12h4M12 12h8M4 17h12M20 17h0" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/><circle cx="16" cy="7" r="2" fill="none" stroke="currentColor" stroke-width="1.8"/><circle cx="10" cy="12" r="2" fill="none" stroke="currentColor" stroke-width="1.8"/><circle cx="18" cy="17" r="2" fill="none" stroke="currentColor" stroke-width="1.8"/>"#,
    };

    pub static SEARCH: Icon = Icon {
        name: "search",
        view: 24.0,
        body: r#"<circle cx="11" cy="11" r="6.5" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="M16 16l4 4" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>"#,
    };

    pub static CHEVRON_DOWN: Icon = Icon {
        name: "chevron_down",
        view: 24.0,
        body: r#"<path d="M6 9l6 6 6-6" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>"#,
    };

    pub static CHECK: Icon = Icon {
        name: "check",
        view: 24.0,
        body: r#"<path d="M5 12.5l4.5 4.5L19 7" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>"#,
    };

    pub static CLOSE: Icon = Icon {
        name: "close",
        view: 24.0,
        body: r#"<path d="M6 6l12 12M18 6L6 18" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>"#,
    };

    pub static MORE_VERT: Icon = Icon {
        name: "more_vert",
        view: 24.0,
        body: r#"<circle cx="12" cy="6" r="1.5" fill="currentColor"/><circle cx="12" cy="12" r="1.5" fill="currentColor"/><circle cx="12" cy="18" r="1.5" fill="currentColor"/>"#,
    };

    pub static PLAY: Icon = Icon {
        name: "play",
        view: 24.0,
        body: r#"<path d="M7 5.5v13l11-6.5-11-6.5z" fill="currentColor"/>"#,
    };

    pub static PAUSE: Icon = Icon {
        name: "pause",
        view: 24.0,
        body: r#"<rect x="6" y="5" width="4" height="14" rx="1" fill="currentColor"/><rect x="14" y="5" width="4" height="14" rx="1" fill="currentColor"/>"#,
    };

    pub static TRASH: Icon = Icon {
        name: "trash",
        view: 24.0,
        body: r#"<path d="M4 7h16M9 7V5h6v2M7 7l1 12h8l1-12" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>"#,
    };

    pub static FOLDER: Icon = Icon {
        name: "folder",
        view: 24.0,
        body: r#"<path d="M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2h8.5A1.5 1.5 0 0 1 21 8.5v9A1.5 1.5 0 0 1 19.5 19h-15A1.5 1.5 0 0 1 3 17.5v-11z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/>"#,
    };

    pub static WARNING: Icon = Icon {
        name: "warning",
        view: 24.0,
        body: r#"<path d="M12 4L2.5 20h19L12 4z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/><path d="M12 10v4.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/><circle cx="12" cy="17.2" r="1" fill="currentColor"/>"#,
    };
}

/// Surface card container with subtle 1px border and rounded corners.
pub fn card(p: &Palette, key: &str) -> El {
    col()
        .key(key)
        .padding(space::LG)
        .gap(space::MD)
        .fill(p.bg_surface)
        .radius(radius::MD)
        .border(1.0, p.border_subtle)
}

/// Structural panel container (sidebars, inspectors, timeline dock).
#[allow(dead_code)]
pub fn panel(p: &Palette, key: &str) -> El {
    col()
        .key(key)
        .fill(p.bg_panel)
        .radius(radius::MD)
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
                .font(typography::MONO_BADGE)
                .color(fg)
                .a11y_hidden(),
        )
}

/// Primary accent button (`#D0BCFF` fill, `#3C0091` dark violet label).
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
        .focus_ring(1.0, 1.0, p.border_focus)
        .cursor(Cursor::Pointer)
        .px(space::MD)
        .h(34.0)
        .center()
        .radius(radius::SM)
        .fill(p.accent_primary)
        .hover(p.accent_hover)
        .press(p.accent_pressed)
        .motion(snappy_spring())
        .on_press(on_press)
        .child(
            text(label_text.to_string())
                .font(typography::LABEL)
                .color(p.on_accent_primary)
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
        .focus_ring(1.0, 1.0, p.border_focus)
        .cursor(Cursor::Pointer)
        .px(space::MD)
        .h(34.0)
        .center()
        .radius(radius::SM)
        .fill(p.bg_surface)
        .border(1.0, p.border_subtle)
        .hover(p.bg_hover)
        .press(p.bg_active)
        .motion(snappy_spring())
        .on_press(on_press)
        .child(
            text(label_text.to_string())
                .font(typography::LABEL)
                .color(p.text_primary)
                .a11y_hidden(),
        )
}

/// Low-emphasis ghost utility button.
#[allow(dead_code)]
pub fn btn_ghost(
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
        .focus_ring(1.0, 1.0, p.border_focus)
        .cursor(Cursor::Pointer)
        .px(space::SM)
        .h(30.0)
        .center()
        .radius(radius::SM)
        .fill(Color::TRANSPARENT)
        .hover(p.bg_hover)
        .press(p.bg_active)
        .motion(snappy_spring())
        .on_press(on_press)
        .child(
            text(label_text.to_string())
                .font(typography::LABEL)
                .color(p.text_secondary)
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
        .focus_ring(1.0, 1.0, p.recording)
        .cursor(Cursor::Pointer)
        .px(space::MD)
        .h(34.0)
        .center()
        .radius(radius::SM)
        .fill(p.recording_soft)
        .border(1.0, p.recording)
        .hover(p.recording)
        .motion(snappy_spring())
        .on_press(on_press)
        .child(
            text(label_text.to_string())
                .font(typography::LABEL)
                .color(p.text_primary)
                .a11y_hidden(),
        )
}

/// Compact square icon button for toolbars and navigation rails.
#[allow(dead_code)]
pub fn icon_btn(
    p: &Palette,
    key: &str,
    ic: &'static Icon,
    label_text: &str,
    active: bool,
    on_press: impl Fn(&mut AppState) + 'static,
) -> El {
    icon_btn_sized(p, key, ic, 36.0, 18.0, label_text, active, on_press)
}

/// Sized square icon button with explicit container and icon dimensions.
#[allow(clippy::too_many_arguments)]
pub fn icon_btn_sized(
    p: &Palette,
    key: &str,
    ic: &'static Icon,
    box_size: f32,
    icon_size: f32,
    label_text: &str,
    active: bool,
    on_press: impl Fn(&mut AppState) + 'static,
) -> El {
    row()
        .key(key)
        .role("button")
        .label(label_text)
        .checked(active)
        .focusable(true)
        .focus_ring(1.0, 1.0, p.border_focus)
        .cursor(Cursor::Pointer)
        .size(box_size, box_size)
        .center()
        .radius(radius::MD)
        .fill(if active {
            p.accent_primary
        } else {
            Color::TRANSPARENT
        })
        .hover(if active { p.accent_hover } else { p.bg_hover })
        .press(if active {
            p.accent_pressed
        } else {
            p.bg_active
        })
        .motion(snappy_spring())
        .on_press(on_press)
        .child(
            icon(ic, icon_size)
                .color(if active {
                    p.on_accent_primary
                } else {
                    p.text_secondary
                })
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
        .focus_ring(1.0, 1.0, p.border_focus)
        .cursor(Cursor::Pointer)
        .px(space::MD)
        .h(30.0)
        .center()
        .radius(radius::SM)
        .fill(if selected {
            p.accent_primary
        } else {
            p.bg_surface
        })
        .border(
            1.0,
            if selected {
                p.accent_primary
            } else {
                p.border_subtle
            },
        )
        .hover(if selected { p.accent_hover } else { p.bg_hover })
        .motion(snappy_spring())
        .on_press(on_press)
        .child(
            text(label_text.to_string())
                .font(typography::LABEL)
                .color(if selected {
                    p.on_accent_primary
                } else {
                    p.text_secondary
                })
                .a11y_hidden(),
        )
}

/// Track container for segmented controls (e.g. Grid/List, capture targets).
#[allow(dead_code)]
pub fn segmented_control(p: &Palette, key: &str, items: Vec<El>) -> El {
    row()
        .key(key)
        .padding(2.0)
        .gap(2.0)
        .center_y()
        .radius(radius::SM)
        .fill(p.bg_lowest)
        .border(1.0, p.border_subtle)
        .children(items)
}

/// Individual option inside a `segmented_control`.
#[allow(dead_code)]
pub fn segmented_item(
    p: &Palette,
    key: &str,
    label_text: &str,
    active: bool,
    on_press: impl Fn(&mut AppState) + 'static,
) -> El {
    row()
        .key(key)
        .role("button")
        .label(label_text)
        .checked(active)
        .focusable(true)
        .cursor(Cursor::Pointer)
        .px(space::SM)
        .h(26.0)
        .center()
        .radius(radius::XS)
        .fill(if active {
            p.accent_primary
        } else {
            Color::TRANSPARENT
        })
        .hover(if active { p.accent_hover } else { p.bg_hover })
        .motion(snappy_spring())
        .on_press(on_press)
        .child(
            text(label_text.to_string())
                .font(typography::LABEL)
                .color(if active {
                    p.on_accent_primary
                } else {
                    p.text_secondary
                })
                .a11y_hidden(),
        )
}

/// Styled single-line `rune::TextEdit` input field.
pub fn text_input_box(
    p: &Palette,
    key: &str,
    edit: &TextEdit,
    placeholder: &str,
    on_edit: impl for<'a> Fn(&'a mut AppState) -> &'a mut TextEdit + 'static,
) -> El {
    text_input_box_inner(p, key, edit, placeholder, false, on_edit)
}

/// Styled single-line masked `rune::TextEdit` input field for passwords and tokens.
pub fn secure_input_box(
    p: &Palette,
    key: &str,
    edit: &TextEdit,
    placeholder: &str,
    on_edit: impl for<'a> Fn(&'a mut AppState) -> &'a mut TextEdit + 'static,
) -> El {
    text_input_box_inner(p, key, edit, placeholder, true, on_edit)
}

fn text_input_box_inner(
    p: &Palette,
    key: &str,
    edit: &TextEdit,
    placeholder: &str,
    secure: bool,
    on_edit: impl for<'a> Fn(&'a mut AppState) -> &'a mut TextEdit + 'static,
) -> El {
    let mut input = text_input(edit, typography::BODY)
        .single_line()
        .key(key)
        .color(p.text_primary)
        .grow(1.0)
        .min_w(0.0)
        .on_edit(on_edit);
    if !placeholder.is_empty() {
        input = input
            .placeholder(placeholder, p.text_muted)
            .label(placeholder);
    }
    if secure {
        input = input.secure();
    }
    row()
        .key(format!("{key}-box"))
        .h(34.0)
        .px(space::SM)
        .center_y()
        .radius(radius::XS)
        .fill(p.bg_lowest)
        .border(1.0, p.border_subtle)
        .focus_within_ring(1.0, 0.0, p.border_focus)
        .child(input)
}

/// Search field with leading magnifying glass icon.
pub fn search_field(
    p: &Palette,
    key: &str,
    edit: &TextEdit,
    placeholder: &str,
    on_edit: impl for<'a> Fn(&'a mut AppState) -> &'a mut TextEdit + 'static,
) -> El {
    let mut input = text_input(edit, typography::BODY)
        .single_line()
        .key(key)
        .color(p.text_primary)
        .grow(1.0)
        .min_w(0.0)
        .on_edit(on_edit);
    if !placeholder.is_empty() {
        input = input
            .placeholder(placeholder, p.text_muted)
            .label(placeholder);
    }
    row()
        .key(format!("{key}-box"))
        .h(34.0)
        .px(space::SM)
        .gap(space::SM)
        .center_y()
        .radius(radius::XS)
        .fill(p.bg_lowest)
        .border(1.0, p.border_subtle)
        .focus_within_ring(1.0, 0.0, p.border_focus)
        .child(icon(&icons::SEARCH, 15.0).color(p.text_muted).a11y_hidden())
        .child(input)
}

/// Compact dropdown/cycle selector trigger with trailing chevron.
#[allow(dead_code)]
pub fn dropdown_select(
    p: &Palette,
    key: &str,
    label_text: &str,
    value_text: &str,
    on_press: impl Fn(&mut AppState) + 'static,
) -> El {
    row()
        .key(key)
        .role("button")
        .label(format!("{label_text}: {value_text}"))
        .focusable(true)
        .focus_ring(1.0, 1.0, p.border_focus)
        .cursor(Cursor::Pointer)
        .h(34.0)
        .px(space::MD)
        .gap(space::SM)
        .center_y()
        .justify_between()
        .radius(radius::SM)
        .fill(p.bg_surface)
        .border(1.0, p.border_subtle)
        .hover(p.bg_hover)
        .press(p.bg_active)
        .motion(snappy_spring())
        .on_press(on_press)
        .child(
            text(value_text.to_string())
                .font(typography::LABEL)
                .color(p.text_primary)
                .a11y_hidden(),
        )
        .child(
            icon(&icons::CHEVRON_DOWN, 14.0)
                .color(p.text_muted)
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
        .focus_ring(1.0, 1.0, p.border_focus)
        .cursor(Cursor::Pointer)
        .size(w, h)
        .radius(h / 2.0)
        .fill(if on { p.accent_primary } else { p.control_off })
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

/// Square 16x16 checkbox for multi-select and boolean options.
#[allow(dead_code)]
pub fn checkbox(
    p: &Palette,
    key: &str,
    label_text: &str,
    checked: bool,
    on_press: impl Fn(&mut AppState) + 'static,
) -> El {
    row()
        .key(key)
        .role("checkbox")
        .label(label_text)
        .checked(checked)
        .focusable(true)
        .focus_ring(1.0, 1.0, p.border_focus)
        .cursor(Cursor::Pointer)
        .size(16.0, 16.0)
        .center()
        .radius(radius::XS)
        .fill(if checked {
            p.accent_primary
        } else {
            p.bg_lowest
        })
        .border(
            1.0,
            if checked {
                p.accent_primary
            } else {
                p.border_active
            },
        )
        .motion(snappy_spring())
        .on_press(on_press)
        .children(if checked {
            Some(
                icon(&icons::CHECK, 12.0)
                    .color(p.on_accent_primary)
                    .a11y_hidden(),
            )
        } else {
            None
        })
}

/// Horizontal tab bar container.
#[allow(dead_code)]
pub fn tab_bar(p: &Palette, key: &str, tabs: Vec<El>) -> El {
    row()
        .key(key)
        .gap(space::XS)
        .pb(space::XS)
        .border_sides([0.0, 0.0, 1.0, 0.0], p.border_subtle)
        .children(tabs)
}

/// Individual tab item for `tab_bar`.
#[allow(dead_code)]
pub fn tab_item(
    p: &Palette,
    key: &str,
    label_text: &str,
    active: bool,
    on_press: impl Fn(&mut AppState) + 'static,
) -> El {
    row()
        .key(key)
        .role("tab")
        .label(label_text)
        .checked(active)
        .focusable(true)
        .cursor(Cursor::Pointer)
        .h(30.0)
        .px(space::MD)
        .center()
        .radius(radius::SM)
        .fill(if active {
            p.accent_soft
        } else {
            Color::TRANSPARENT
        })
        .border(
            1.0,
            if active {
                p.accent_brand
            } else {
                Color::TRANSPARENT
            },
        )
        .hover(if active { p.accent_soft } else { p.bg_hover })
        .motion(snappy_spring())
        .on_press(on_press)
        .child(
            text(label_text.to_string())
                .font(typography::LABEL)
                .color(if active {
                    p.text_primary
                } else {
                    p.text_secondary
                })
                .a11y_hidden(),
        )
}

/// Live telemetry status pill with colored dot, monospace label, and optional hotkey chip.
pub fn status_pill(
    p: &Palette,
    key: &str,
    dot_color: Color,
    label_text: &str,
    fg: Color,
    bg: Color,
    hotkey: Option<&str>,
) -> El {
    row()
        .key(key)
        .h(26.0)
        .px(space::SM)
        .gap(space::SM)
        .center_y()
        .radius(radius::PILL)
        .fill(bg)
        .border(1.0, p.border_subtle)
        .child(
            el().key("dot")
                .size(7.0, 7.0)
                .radius(3.5)
                .fill(dot_color)
                .a11y_hidden(),
        )
        .child(
            text(label_text.to_string())
                .font(typography::MONO_METRIC)
                .color(fg),
        )
        .children(hotkey.map(|hk| {
            row()
                .key("hotkey")
                .px(6.0)
                .py(2.0)
                .radius(radius::XS)
                .fill(p.bg_lowest)
                .border(1.0, p.border_subtle)
                .child(
                    text(hk.to_string())
                        .font(typography::MONO_BADGE)
                        .color(p.text_secondary),
                )
        }))
}

/// Determinate progress bar (`progress` clamped to `0.0..=1.0`).
#[allow(dead_code)]
pub fn progress_bar(p: &Palette, key: &str, progress: f32, fill_color: Color) -> El {
    let pct = progress.clamp(0.0, 1.0);
    row()
        .key(key)
        .w_full()
        .h(6.0)
        .radius(radius::PILL)
        .fill(p.bg_lowest)
        .border(1.0, p.border_subtle)
        .child(
            el().key("fill")
                .h_full()
                .grow(pct)
                .radius(radius::PILL)
                .fill(fill_color),
        )
        .child(el().key("rem").h_full().grow(1.0 - pct))
}

/// Styled container frame for 16:9 clip cards.
#[allow(dead_code)]
pub fn clip_card_frame(p: &Palette, key: &str, selected: bool) -> El {
    col()
        .key(key)
        .radius(radius::MD)
        .fill(if selected {
            p.bg_elevated
        } else {
            p.bg_surface
        })
        .border(
            1.0,
            if selected {
                p.border_focus
            } else {
                p.border_subtle
            },
        )
        .hover(p.bg_hover)
        .motion(snappy_spring())
}

/// Right-hand 320px contextual inspector panel with pinned header and optional footer.
#[allow(dead_code)]
pub fn inspector_panel(
    p: &Palette,
    key: &str,
    title: &str,
    on_close: impl Fn(&mut AppState) + 'static,
    body: El,
    footer: Option<El>,
) -> El {
    col()
        .key(key)
        .w(320.0)
        .h_full()
        .fill(p.bg_panel)
        .border_sides([0.0, 0.0, 0.0, 1.0], p.border_subtle)
        .child(
            row()
                .key("inspector-header")
                .w_full()
                .h(44.0)
                .px(space::LG)
                .center_y()
                .justify_between()
                .border_sides([0.0, 0.0, 1.0, 0.0], p.border_subtle)
                .child(
                    text(title.to_string())
                        .font(typography::MONO_BADGE)
                        .color(p.text_muted),
                )
                .child(icon_btn_sized(
                    p,
                    "inspector-close",
                    &icons::CLOSE,
                    26.0,
                    14.0,
                    "Close inspector",
                    false,
                    on_close,
                )),
        )
        .child(body.grow(1.0).min_h(0.0))
        .children(footer)
}

/// Floating context menu shell positioned at `(x, y)`.
pub fn context_menu_shell(p: &Palette, key: &str, x: f32, y: f32, items: Vec<El>) -> El {
    col()
        .key(key)
        .absolute(x, y)
        .w(210.0)
        .padding(space::XS)
        .gap(2.0)
        .fill(p.bg_elevated)
        .border(1.0, p.border_active)
        .radius(radius::MD)
        .shadow(8.0, 20.0, Color::BLACK.alpha(0.55))
        .children(items)
}

/// Modal confirmation dialog over a dark translucent scrim.
#[allow(dead_code, clippy::too_many_arguments)]
pub fn confirm_dialog(
    p: &Palette,
    key: &str,
    title: &str,
    message: &str,
    confirm_label: &str,
    danger: bool,
    on_confirm: impl Fn(&mut AppState) + 'static,
    on_cancel: impl Fn(&mut AppState) + 'static,
) -> El {
    el().key(key)
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .center()
        .fill(Color::BLACK.alpha(0.65))
        .child(
            col()
                .key("confirm-card")
                .w(420.0)
                .padding(space::XL)
                .gap(space::LG)
                .fill(p.bg_elevated)
                .border(1.0, p.border_active)
                .radius(radius::LG)
                .shadow(12.0, 32.0, Color::BLACK.alpha(0.65))
                .child(
                    col()
                        .key("confirm-copy")
                        .gap(space::SM)
                        .child(
                            text(title.to_string())
                                .font(typography::HEADLINE_SM)
                                .color(p.text_primary),
                        )
                        .child(
                            text(message.to_string())
                                .font(typography::BODY)
                                .color(p.text_secondary),
                        ),
                )
                .child(
                    row()
                        .key("confirm-actions")
                        .w_full()
                        .gap(space::SM)
                        .justify_end()
                        .child(btn_secondary(p, "confirm-cancel", "Cancel", on_cancel))
                        .child(if danger {
                            btn_danger(p, "confirm-ok", confirm_label, on_confirm)
                        } else {
                            btn_primary(p, "confirm-ok", confirm_label, on_confirm)
                        }),
                ),
        )
}

/// Centered empty state card with optional primary action.
#[allow(dead_code)]
pub fn empty_state(p: &Palette, key: &str, title: &str, message: &str, action: Option<El>) -> El {
    col()
        .key(key)
        .w_full()
        .padding(space::XXL)
        .gap(space::MD)
        .center()
        .fill(p.bg_panel)
        .border(1.0, p.border_subtle)
        .radius(radius::LG)
        .child(
            text(title.to_string())
                .font(typography::HEADLINE_SM)
                .color(p.text_primary),
        )
        .child(
            text(message.to_string())
                .font(typography::BODY)
                .color(p.text_secondary),
        )
        .children(action)
}

/// Centered error state card with optional recovery action.
#[allow(dead_code)]
pub fn error_state(p: &Palette, key: &str, title: &str, message: &str, action: Option<El>) -> El {
    col()
        .key(key)
        .w_full()
        .padding(space::XL)
        .gap(space::MD)
        .center()
        .fill(p.bg_panel)
        .border(1.0, p.recording)
        .radius(radius::LG)
        .child(
            row()
                .key("err-head")
                .gap(space::SM)
                .center_y()
                .child(icon(&icons::WARNING, 18.0).color(p.recording).a11y_hidden())
                .child(
                    text(title.to_string())
                        .font(typography::HEADLINE_SM)
                        .color(p.error_text),
                ),
        )
        .child(
            text(message.to_string())
                .font(typography::BODY)
                .color(p.text_secondary),
        )
        .children(action)
}

/// Centered loading / active-operation card with optional determinate progress bar.
#[allow(dead_code)]
pub fn loading_state(
    p: &Palette,
    key: &str,
    title: &str,
    message: &str,
    progress: Option<f32>,
) -> El {
    col()
        .key(key)
        .w_full()
        .padding(space::XL)
        .gap(space::MD)
        .center()
        .fill(p.bg_panel)
        .border(1.0, p.border_subtle)
        .radius(radius::LG)
        .child(
            text(title.to_string())
                .font(typography::HEADLINE_SM)
                .color(p.text_primary),
        )
        .child(
            text(message.to_string())
                .font(typography::BODY)
                .color(p.text_secondary),
        )
        .children(progress.map(|v| {
            el().key("loading-bar").w(280.0).child(progress_bar(
                p,
                "loading-progress",
                v,
                p.accent_primary,
            ))
        }))
}

/// Settings row with title, muted description, and right-aligned control.
#[allow(dead_code)]
pub fn settings_row(p: &Palette, key: &str, title: &str, description: &str, control: El) -> El {
    row()
        .key(key)
        .w_full()
        .py(space::SM)
        .gap(space::LG)
        .justify_between()
        .center_y()
        .child(
            col()
                .key("meta")
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(
                    text(title.to_string())
                        .font(typography::BODY)
                        .color(p.text_primary),
                )
                .child(
                    text(description.to_string())
                        .font(typography::LABEL)
                        .color(p.text_muted),
                ),
        )
        .child(control)
}

/// Dashboard / telemetry KPI tile.
pub fn stat_tile(p: &Palette, key: &str, label_text: &str, value: String, hint: &str) -> El {
    card(p, key)
        .grow(1.0)
        .gap(space::XS)
        .child(
            text(label_text.to_string())
                .font(typography::LABEL)
                .color(p.text_secondary),
        )
        .child(
            text(value)
                .font(typography::HEADLINE_MD)
                .color(p.text_primary),
        )
        .child(
            text(hint.to_string())
                .font(typography::LABEL)
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
                .font(typography::HEADLINE_SM)
                .color(p.text_primary),
        )
        .child(
            text(subtitle.to_string())
                .font(typography::LABEL)
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
                .font(typography::BODY)
                .color(p.text_secondary),
        )
        .child(
            text(value)
                .font(typography::MONO_METRIC)
                .color(p.text_primary),
        )
}
