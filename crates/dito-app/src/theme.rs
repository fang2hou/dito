//! Design tokens from the FigCraft design (`10 Color`), light and dark.

use dito_core::config::Appearance;
use gpui_kit::{App, Hsla, Rgba, WindowAppearance};

/// `#RRGGBB` or `#RRGGBBAA`.
pub fn hex(s: &str) -> Hsla {
    let h = s.trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).expect("hex color");
    let (r, g, b, a) = if h.len() == 8 {
        ((v >> 24) & 0xff, (v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff)
    } else {
        ((v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff, 0xff)
    };
    Rgba { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: a as f32 / 255.0 }.into()
}

pub const PLAIN_BLACK: &str = "#444139";

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg: Hsla,
    pub side: Hsla,
    pub card: Hsla,
    pub sub: Hsla,
    pub border: Hsla,
    pub border_strong: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub primary: Hsla,
    pub on_primary: Hsla,
    pub accent: Hsla,
    pub accent_text: Hsla,
    pub accent_soft: Hsla,
    pub danger: Hsla,
    pub danger_text: Hsla,
    pub danger_soft: Hsla,
    pub success: Hsla,
    pub success_text: Hsla,
    pub success_soft: Hsla,
    /// Label tints for history states: warning (cancelled) and blue (dictation style identity).
    /// Muted chips in both themes — failures keep the loud solid red, these stay quiet.
    pub warning_text: Hsla,
    pub warning_soft: Hsla,
    pub blue_text: Hsla,
    pub blue_soft: Hsla,
    pub seg: Hsla,
    pub seg_on: Hsla,
    /// Idle model-tile fill: a softer recess than `seg`, which reads too heavy on a white card.
    pub tile_idle: Hsla,
    /// Model-row plates. Ghost (not downloaded) < idle < hover (`tile_idle`) < active (in use).
    pub row_idle: Hsla,
    pub row_active: Hsla,
    /// Popup-menu surface: white in light, near-black in dark (macOS-style dropdown).
    pub menu_bg: Hsla,
    pub toggle_off: Hsla,
    pub knob_on: Hsla,
    pub knob_off: Hsla,
    pub nav_on: Hsla,
    /// Hairline that replaces section headers in the collapsed icon rail.
    pub hair: Hsla,
    pub input_bg: Hsla,
    pub hud_bg: Hsla,
    pub hud_border: Hsla,
    pub shadow: Hsla,
}

impl Palette {
    pub fn light() -> Self {
        Self {
            dark: false,
            bg: hex("#FFFFFF"),
            side: hex("#F7F5F0"),
            card: hex("#FFFFFF"),
            sub: hex("#F7F5F0"),
            border: hex("#EBE8E1"),
            border_strong: hex("#D4D1C9"),
            text: hex("#444139"),
            muted: hex("#6A6455"),
            primary: hex("#444139"),
            on_primary: hex("#FFFFFF"),
            accent: hex("#AA8C40"),
            accent_text: hex("#826F41"),
            accent_soft: hex("#FFF9EB"),
            danger: hex("#FF4949"),
            danger_text: hex("#FF4949"),
            danger_soft: hex("#FFEFEF"),
            success: hex("#28A788"),
            success_text: hex("#28A788"),
            success_soft: hex("#EBF6F3"),
            warning_text: hex("#8F6A1E"),
            warning_soft: hex("#F9EFD8"),
            blue_text: hex("#4674A8"),
            blue_soft: hex("#E9F0F7"),
            seg: hex("#EBE8E1"),
            seg_on: hex("#FFFFFF"),
            tile_idle: hex("#F1EFE9"),
            row_idle: hex("#FAF9F5"),
            row_active: hex("#EBE8E1"),
            menu_bg: hex("#FFFFFF"),
            toggle_off: hex("#D4D1C9"),
            knob_on: hex("#FFFFFF"),
            knob_off: hex("#FFFFFF"),
            nav_on: hex("#EBE8E1"),
            hair: hex("#D9D6CD"),
            input_bg: hex("#FFFFFF"),
            hud_bg: hex("#FFFFFF"),
            hud_border: hex("#D4D1C9"),
            shadow: hex("#4441391F"),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: hex("#444139"),
            side: hex("#38352F"),
            card: hex("#4D493F"),
            sub: hex("#3E3B34"),
            border: hex("#5A564B"),
            border_strong: hex("#6A6455"),
            text: hex("#F7F5F0"),
            muted: hex("#C2BDB2"),
            primary: hex("#E0C47D"),
            on_primary: hex("#444139"),
            accent: hex("#E0C47D"),
            accent_text: hex("#E0C47D"),
            accent_soft: hex("#5B5340"),
            danger: hex("#FF4949"),
            danger_text: hex("#FFFFFF"),
            danger_soft: hex("#FF4949"),
            success: hex("#28A788"),
            success_text: hex("#FFFFFF"),
            success_soft: hex("#28A788"),
            warning_text: hex("#E5C268"),
            warning_soft: hex("#5B5340"),
            blue_text: hex("#A9C6E4"),
            blue_soft: hex("#3E4A57"),
            seg: hex("#38352F"),
            seg_on: hex("#5A564B"),
            tile_idle: hex("#38352F"),
            row_idle: hex("#423F38"),
            row_active: hex("#2F2C26"),
            menu_bg: hex("#2F2C26"),
            toggle_off: hex("#6A6455"),
            knob_on: hex("#444139"),
            knob_off: hex("#F7F5F0"),
            nav_on: hex("#5A564B"),
            hair: hex("#514D43"),
            input_bg: hex("#3E3B34"),
            hud_bg: hex("#444139"),
            hud_border: hex("#6A6455"),
            shadow: hex("#00000066"),
        }
    }

    pub fn resolve(setting: Appearance, system: WindowAppearance) -> Self {
        let dark = match setting {
            Appearance::Light => false,
            Appearance::Dark => true,
            Appearance::System => matches!(system, WindowAppearance::Dark | WindowAppearance::VibrantDark),
        };
        if dark { Self::dark() } else { Self::light() }
    }

    /// Text color on tinted status backgrounds (the design uses solid accents with white text in dark mode).
    pub fn on_tint(&self, kind: Tint) -> (Hsla, Hsla) {
        match kind {
            Tint::Danger => (self.danger_soft, self.danger_text),
            Tint::Warning => (self.warning_soft, self.warning_text),
            Tint::Blue => (self.blue_soft, self.blue_text),
            Tint::Neutral => (self.seg, self.muted),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tint {
    Danger,
    // Reserved by the unwired HistoryKind::Cancelled meta tag.
    #[allow(dead_code)]
    Warning,
    Blue,
    Neutral,
}

/// Push the palette into gpui-component so its inputs match the design.
pub fn apply(p: &Palette, cx: &mut App) {
    use gpui_kit::component::Theme;
    let t = Theme::global_mut(cx);
    t.colors.background = p.bg;
    t.colors.foreground = p.text;
    t.colors.muted_foreground = p.muted;
    t.colors.caret = p.accent;
    t.colors.selection = hex_alpha(p.accent, 0.3);
    t.colors.input = p.border_strong;
    t.colors.border = p.border;
    t.colors.ring = p.accent;
    t.colors.primary = p.primary;
    t.font_family = crate::ui::cjk_family().into();
    t.font_size = gpui_kit::px(13.);
}

fn hex_alpha(c: Hsla, a: f32) -> Hsla {
    Hsla { a, ..c }
}

pub fn palette(setting: Appearance, cx: &App) -> Palette {
    Palette::resolve(setting, cx.window_appearance())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parses_rgb_and_rgba() {
        let c: Rgba = hex("#FF0000").into();
        assert_eq!((c.r, c.g, c.b, c.a), (1.0, 0.0, 0.0, 1.0));
        let c: Rgba = hex("#00000080").into();
        assert!((c.a - 128.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn system_follows_the_os_appearance_and_explicit_choice_wins() {
        assert!(Palette::resolve(Appearance::System, WindowAppearance::Dark).dark);
        assert!(!Palette::resolve(Appearance::System, WindowAppearance::Light).dark);
        assert!(!Palette::resolve(Appearance::Light, WindowAppearance::Dark).dark);
        assert!(Palette::resolve(Appearance::Dark, WindowAppearance::Light).dark);
    }
}
