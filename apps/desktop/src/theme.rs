//! Design system tokens for ClipForge Desktop (Linear / Raycast / Medal.tv inspired, dark-first).

use rune::{Color, Spring, oklch, oklcha};

/// Spacing scale in logical points.
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
    pub const SM: f32 = 6.0;
    pub const MD: f32 = 8.0;
    pub const LG: f32 = 12.0;
    pub const PILL: f32 = 999.0;
}

/// Restrained spring presets for interactive controls.
pub fn snappy_spring() -> Spring {
    Spring::new(0.22, 0.0)
}

/// Semantic color palette for dark-first ClipForge UI.
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg_base: Color,
    pub bg_sidebar: Color,
    pub bg_surface: Color,
    pub bg_elevated: Color,
    pub bg_hover: Color,
    pub bg_active: Color,
    pub border_subtle: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub accent: Color,
    pub accent_soft: Color,
    pub accent_text: Color,
    pub recording: Color,
    pub recording_soft: Color,
    pub success: Color,
    pub success_soft: Color,
    pub warning: Color,
    pub warning_soft: Color,
    pub control_off: Color,
    pub knob: Color,
}

impl Palette {
    /// Returns the primary dark-first ClipForge palette.
    pub fn dark() -> Self {
        Self {
            bg_base: oklch(0.14, 0.015, 260.0),
            bg_sidebar: oklch(0.17, 0.016, 260.0),
            bg_surface: oklch(0.20, 0.018, 260.0),
            bg_elevated: oklch(0.24, 0.020, 260.0),
            bg_hover: oklch(0.27, 0.022, 260.0),
            bg_active: oklch(0.30, 0.030, 262.0),
            border_subtle: oklcha(0.45, 0.02, 260.0, 0.22),
            text_primary: oklch(0.96, 0.008, 260.0),
            text_secondary: oklch(0.75, 0.018, 260.0),
            text_muted: oklch(0.56, 0.018, 260.0),
            accent: oklch(0.64, 0.21, 268.0),
            accent_soft: oklcha(0.64, 0.21, 268.0, 0.16),
            accent_text: oklch(0.98, 0.01, 268.0),
            recording: oklch(0.63, 0.24, 24.0),
            recording_soft: oklcha(0.63, 0.24, 24.0, 0.18),
            success: oklch(0.72, 0.17, 152.0),
            success_soft: oklcha(0.72, 0.17, 152.0, 0.16),
            warning: oklch(0.78, 0.16, 75.0),
            warning_soft: oklcha(0.78, 0.16, 75.0, 0.16),
            control_off: oklch(0.30, 0.015, 260.0),
            knob: oklch(0.98, 0.0, 0.0),
        }
    }
}
