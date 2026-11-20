//! Obsidian Forge design system tokens for ClipForge Desktop.

use rune::tok::{Family, TextStyle};
use rune::{Color, Spring};

/// Spacing scale in logical points (4px base grid).
pub mod space {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 24.0;
    pub const XXL: f32 = 32.0;
}

/// Corner radius scale in logical points.
pub mod radius {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 6.0;
    pub const MD: f32 = 8.0;
    pub const LG: f32 = 12.0;
    pub const PILL: f32 = 999.0;
}

/// Canonical Obsidian Forge typography presets (Inter + JetBrains Mono).
#[allow(dead_code)]
pub mod typography {
    use super::*;

    pub const HEADLINE_LG: TextStyle = TextStyle::new(22.0, 600.0)
        .family(Family::Sans)
        .lh(28.0)
        .tracking(-0.02);
    pub const HEADLINE_MD: TextStyle = TextStyle::new(18.0, 600.0)
        .family(Family::Sans)
        .lh(24.0)
        .tracking(-0.015);
    pub const HEADLINE_SM: TextStyle = TextStyle::new(15.0, 600.0)
        .family(Family::Sans)
        .lh(20.0)
        .tracking(-0.01);
    pub const BODY: TextStyle = TextStyle::new(13.0, 400.0).family(Family::Sans).lh(18.0);
    pub const LABEL: TextStyle = TextStyle::new(12.0, 500.0)
        .family(Family::Sans)
        .lh(16.0)
        .tracking(0.01);
    pub const MICRO: TextStyle = TextStyle::new(11.0, 500.0)
        .family(Family::Sans)
        .lh(14.0)
        .tracking(0.02);
    pub const MONO_METRIC: TextStyle = TextStyle::new(12.0, 500.0).mono().lh(16.0).tracking(0.02);
    pub const MONO_BADGE: TextStyle = TextStyle::new(11.0, 500.0).mono().lh(14.0).tracking(0.04);
}

/// Restrained spring preset for interactive controls (no overshoot).
pub fn snappy_spring() -> Spring {
    Spring::new(0.22, 0.0)
}

fn srgb_to_linear(c: u8) -> f32 {
    let s = c as f32 / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb(code: &str) -> Color {
    let h = code.strip_prefix('#').unwrap_or(code);
    if h.len() == 6
        && let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&h[0..2], 16),
            u8::from_str_radix(&h[2..4], 16),
            u8::from_str_radix(&h[4..6], 16),
        )
    {
        return Color {
            r: srgb_to_linear(r),
            g: srgb_to_linear(g),
            b: srgb_to_linear(b),
            a: 1.0,
        };
    }
    Color::WHITE
}

/// Canonical Obsidian Forge color palette for ClipForge Desktop.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct Palette {
    // 7-tier surface hierarchy
    pub bg_lowest: Color,
    pub bg_base: Color,
    pub bg_panel: Color,
    pub bg_sidebar: Color,
    pub bg_surface: Color,
    pub bg_elevated: Color,
    pub bg_hover: Color,
    pub bg_active: Color,

    // Borders & dividers
    pub border_subtle: Color,
    pub border_active: Color,
    pub border_focus: Color,

    // Text hierarchy
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_disabled: Color,

    // Violet accent family
    pub accent_primary: Color,
    pub on_accent_primary: Color,
    pub accent_hover: Color,
    pub accent_brand: Color,
    pub accent: Color,
    pub accent_soft: Color,
    pub accent_glow: Color,
    pub accent_pressed: Color,
    pub accent_text: Color,

    // Semantic & telemetry colors
    pub recording: Color,
    pub recording_soft: Color,
    pub error_text: Color,
    pub error_container: Color,
    pub success: Color,
    pub success_text: Color,
    pub success_soft: Color,
    pub warning: Color,
    pub warning_text: Color,
    pub warning_soft: Color,

    // Controls
    pub control_off: Color,
    pub knob: Color,
}

impl Palette {
    /// Returns the canonical Obsidian Forge dark palette.
    pub fn dark() -> Self {
        let accent_brand = srgb("#8B5CF6");
        let recording = srgb("#EF4444");
        let success = srgb("#10B981");
        let warning = srgb("#F59E0B");
        let bg_lowest = srgb("#0D0E13");
        let bg_active = srgb("#34343A");

        Self {
            bg_lowest,
            bg_base: srgb("#121318"),
            bg_panel: srgb("#181920"),
            bg_sidebar: bg_lowest,
            bg_surface: srgb("#1C1E27"),
            bg_elevated: srgb("#232634"),
            bg_hover: srgb("#292A2F"),
            bg_active,

            border_subtle: Color::WHITE.alpha(0.08),
            border_active: srgb("#2A2D3D"),
            border_focus: accent_brand,

            text_primary: srgb("#E3E1E9"),
            text_secondary: srgb("#CBC3D7"),
            text_muted: srgb("#958EA0"),
            text_disabled: srgb("#5E5968"),

            accent_primary: srgb("#D0BCFF"),
            on_accent_primary: srgb("#3C0091"),
            accent_hover: srgb("#A078FF"),
            accent_brand,
            accent: accent_brand,
            accent_soft: accent_brand.alpha(0.16),
            accent_glow: accent_brand.alpha(0.15),
            accent_pressed: srgb("#6D3BD7"),
            accent_text: Color::WHITE,

            recording,
            recording_soft: recording.alpha(0.16),
            error_text: srgb("#FFB4AB"),
            error_container: srgb("#93000A"),
            success,
            success_text: srgb("#4EDEA3"),
            success_soft: success.alpha(0.16),
            warning,
            warning_text: srgb("#FFB95F"),
            warning_soft: warning.alpha(0.16),

            control_off: bg_active,
            knob: Color::WHITE,
        }
    }
}
