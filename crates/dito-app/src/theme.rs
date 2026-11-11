//! Design tokens from the FigCraft design (`design:foundations/color`), light and dark.

use dito_core::config::Appearance;
use gpui_kit::{App, Hsla, Rgba, WindowAppearance};

use crate::ui::hex_alpha;

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
    /// Muted chips in both themes: failures keep the loud solid red, these stay quiet.
    pub warning_text: Hsla,
    pub warning_soft: Hsla,
    pub blue_text: Hsla,
    pub blue_soft: Hsla,
    pub seg: Hsla,
    pub seg_on: Hsla,
    /// Softer than `seg`, which reads too heavy on a white card.
    pub tile_idle: Hsla,
    /// Plate behind a model card while its download runs, verifies or has failed.
    pub row_idle: Hsla,
    /// Surface of the sidebar health pop-up.
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
    /// Component tokens from the design file's `Color` collection (`button/…`, `select/…`, `model/…`).
    pub button_primary_hover: Hsla,
    pub button_danger_bg: Hsla,
    pub button_danger_border: Hsla,
    pub button_danger_text: Hsla,
    pub button_danger_hover: Hsla,
    pub select_hover: Hsla,
    pub select_hover_border: Hsla,
    pub segment_hover: Hsla,
    pub segment_hover_label: Hsla,
    pub switch_knob_shadow: Hsla,
    pub slider_thumb_hover: Hsla,
    pub slider_thumb_shadow: Hsla,
    pub slider_thumb_shadow_pressed: Hsla,
    pub model_meta: Hsla,
    pub model_download_bg: Hsla,
    pub model_download_border: Hsla,
    pub model_idle_bg: Hsla,
    pub model_idle_border: Hsla,
    pub model_idle_border_hover: Hsla,
    pub model_badge_text: Hsla,
    pub model_error_text: Hsla,
    pub model_in_use_bg: Hsla,
    pub model_in_use_border: Hsla,
    pub model_running_bg: Hsla,
    pub model_running_text: Hsla,
    pub field_bg: Hsla,
    pub field_border: Hsla,
    /// Heat steps of the Stats daily view (`stats/heat_0` to `stats/heat_4`): no dictation, then four quartiles.
    pub stats_heat_0: Hsla,
    pub stats_heat_1: Hsla,
    pub stats_heat_2: Hsla,
    pub stats_heat_3: Hsla,
    pub stats_heat_4: Hsla,
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
            primary: hex("#AA8C40"),
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
            button_primary_hover: hex("#FFFFFF29"),
            button_danger_bg: hex("#FFF0EC"),
            button_danger_border: hex("#D99588"),
            button_danger_text: hex("#A44232"),
            button_danger_hover: hex("#A4423224"),
            select_hover: hex("#EBE8E18C"),
            select_hover_border: hex("#6A645580"),
            segment_hover: hex("#FFFFFF8C"),
            segment_hover_label: hex("#534F44"),
            switch_knob_shadow: hex("#0000000D"),
            slider_thumb_hover: hex("#F0EEE9"),
            slider_thumb_shadow: hex("#0000002E"),
            slider_thumb_shadow_pressed: hex("#00000066"),
            model_meta: hex("#6A6455"),
            model_download_bg: hex("#FAF9F5"),
            model_download_border: hex("#EBE8E1"),
            model_idle_bg: hex("#F7F5F0"),
            model_idle_border: hex("#D4D1C9"),
            model_idle_border_hover: hex("#B8AE9C"),
            model_badge_text: hex("#6A6455"),
            model_error_text: hex("#444139"),
            model_in_use_bg: hex("#F6EED8"),
            model_in_use_border: hex("#977326"),
            model_running_bg: hex("#E4F2E9"),
            model_running_text: hex("#17634F"),
            field_bg: hex("#FFFFFF"),
            field_border: hex("#D4D1C9"),
            stats_heat_0: hex("#EBE8E1"),
            stats_heat_1: hex("#FFECBD"),
            stats_heat_2: hex("#F6DA97"),
            stats_heat_3: hex("#C2A765"),
            stats_heat_4: hex("#AA8C40"),
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
            button_primary_hover: hex("#00000029"),
            button_danger_bg: hex("#5A3831"),
            button_danger_border: hex("#A76C5C"),
            button_danger_text: hex("#FFBAA7"),
            button_danger_hover: hex("#FFBAA738"),
            select_hover: hex("#38352F8C"),
            select_hover_border: hex("#C2BDB280"),
            segment_hover: hex("#5A564B8C"),
            segment_hover_label: hex("#E2DFD7"),
            switch_knob_shadow: hex("#00000033"),
            slider_thumb_hover: hex("#524F46"),
            slider_thumb_shadow: hex("#0000002E"),
            slider_thumb_shadow_pressed: hex("#00000066"),
            model_meta: hex("#C9C3B7"),
            model_download_bg: hex("#423F38"),
            model_download_border: hex("#5A564B"),
            model_idle_bg: hex("#423F38"),
            model_idle_border: hex("#716B5E"),
            model_idle_border_hover: hex("#918775"),
            model_badge_text: hex("#C9C3B7"),
            model_error_text: hex("#F7F5F0"),
            model_in_use_bg: hex("#4B422E"),
            model_in_use_border: hex("#E0C47D"),
            model_running_bg: hex("#263F34"),
            model_running_text: hex("#80E1BF"),
            field_bg: hex("#38352F"),
            field_border: hex("#716B5E"),
            stats_heat_0: hex("#3E3B34"),
            stats_heat_1: hex("#685218"),
            stats_heat_2: hex("#826F41"),
            stats_heat_3: hex("#AA8C40"),
            stats_heat_4: hex("#E0C47D"),
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

    /// In dark mode the design uses solid accents with white text.
    pub fn on_tint(&self, kind: Tint) -> (Hsla, Hsla) {
        match kind {
            Tint::Danger => (self.danger_soft, self.danger_text),
            Tint::Warning => (self.warning_soft, self.warning_text),
            Tint::Blue => (self.blue_soft, self.blue_text),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tint {
    Danger,
    Warning,
    Blue,
}

/// Push the palette into gpui-component so its inputs match the design.
pub fn apply(p: &Palette, cx: &mut App) {
    use crate::ui::Theme;
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

/// A HUD color: the HUD keeps its own light and dark values, independent from the settings window's palette.
pub fn hud_color(p: &Palette, light: &str, dark: &str) -> Hsla {
    hex(if p.dark { dark } else { light })
}

pub fn hud_stop_ink(p: &Palette) -> Hsla {
    hud_color(p, "#DD202C", "#FF7972")
}

pub fn hud_error_bg(p: &Palette) -> Hsla {
    hud_color(p, "#FFEFEF", "#62423C")
}

pub fn hud_error_pill_border() -> Hsla {
    hex("#FF494966")
}

pub fn hud_lang_plate(p: &Palette) -> Hsla {
    hud_color(p, "#FFECBD", "#E0C47D")
}

pub fn hud_lang_muted(p: &Palette) -> Hsla {
    hud_color(p, "#6A6455", "#D4D1C9")
}

/// The recording countdown pill once ten seconds or fewer remain.
pub fn hud_countdown_warn_bg(p: &Palette) -> Hsla {
    hud_color(p, "#FFF3D9", "#51452D")
}

pub fn hud_countdown_warn_border(p: &Palette) -> Hsla {
    hud_color(p, "#D9BA77", "#9C8047")
}

pub fn hud_countdown_warn_ink(p: &Palette) -> Hsla {
    hud_color(p, "#805A17", "#F1CF83")
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
        assert!(!Palette::resolve(Appearance::Light, WindowAppearance::Dark).dark);
    }
}
