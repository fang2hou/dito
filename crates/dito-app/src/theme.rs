//! Design tokens from the FigCraft design (`design:foundations/color`), light and dark.

use std::sync::LazyLock;

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

#[derive(Clone, Copy, PartialEq)]
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
    /// The plate of a completed requirement row: a green tint on the card that keeps its text readable in both
    /// themes, unlike `success_soft` which is a solid chip in the dark palette.
    pub success_row: Hsla,
    /// macOS microphone orange, used by the microphone source toggle and its tinted surfaces.
    pub orange: Hsla,
    /// The quiet and the readable text step of the microphone orange, from the same color collection.
    #[allow(dead_code, reason = "the color collection's microphone pair, for mic surfaces shared with Status")]
    pub orange_soft: Hsla,
    #[allow(dead_code, reason = "the color collection's microphone pair, for mic surfaces shared with Status")]
    pub orange_text: Hsla,
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
    /// The ring that flashes around the armed stop pill (`button/danger_halo`).
    pub button_danger_halo: Hsla,
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
    pub field_bg: Hsla,
    pub field_border: Hsla,
    /// Command mode (`design:hud/command`): the ocean accent of its windows and their actions.
    pub command_primary: Hsla,
    pub command_primary_hover: Hsla,
    /// Tinted surfaces: the likeliest choice, the result box.
    pub command_soft: Hsla,
    pub command_soft_hover: Hsla,
    /// The glow under floating action buttons.
    pub command_glow: Hsla,
    /// Heat steps of the Stats daily view (`stats/heat_0` to `stats/heat_4`): no dictation, then four quartiles.
    pub stats_heat_0: Hsla,
    pub stats_heat_1: Hsla,
    pub stats_heat_2: Hsla,
    pub stats_heat_3: Hsla,
    pub stats_heat_4: Hsla,
}

static LIGHT: LazyLock<Palette> = LazyLock::new(Palette::build_light);
static DARK: LazyLock<Palette> = LazyLock::new(Palette::build_dark);

impl Palette {
    pub fn light() -> Self {
        *LIGHT
    }

    pub fn dark() -> Self {
        *DARK
    }

    fn build_light() -> Self {
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
            success_row: hex("#EBF6F3"),
            orange: hex("#E87B2E"),
            orange_soft: hex("#FDF1E7"),
            orange_text: hex("#C25E16"),
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
            button_danger_halo: hex("#D995888C"),
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
            field_bg: hex("#FFFFFF"),
            field_border: hex("#D4D1C9"),
            command_primary: hex("#3F72AB"),
            command_primary_hover: hex("#5D90C4"),
            command_soft: hex("#EEF5FB"),
            command_soft_hover: hex("#CFE2F5"),
            command_glow: hex("#3F72AB73"),
            stats_heat_0: hex("#EBE8E1"),
            stats_heat_1: hex("#FFECBD"),
            stats_heat_2: hex("#F6DA97"),
            stats_heat_3: hex("#C2A765"),
            stats_heat_4: hex("#AA8C40"),
        }
    }

    fn build_dark() -> Self {
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
            success_row: hex("#394842"),
            orange: hex("#E87B2E"),
            orange_soft: hex("#5C4330"),
            orange_text: hex("#FFB37B"),
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
            button_danger_halo: hex("#FFBAA759"),
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
            field_bg: hex("#38352F"),
            field_border: hex("#716B5E"),
            command_primary: hex("#7FAEDC"),
            command_primary_hover: hex("#A3C7EA"),
            command_soft: hex("#3E4A57"),
            command_soft_hover: hex("#4A5B70"),
            command_glow: hex("#7FAEDC8C"),
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
            Tint::Command => (self.command_soft, self.command_primary),
            Tint::Neutral if self.dark => (hex("#46515A"), hex("#C1D6E4")),
            Tint::Neutral => (self.seg, self.muted),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tint {
    Danger,
    Warning,
    Blue,
    /// The command tag of a History row.
    Command,
    /// A quiet tag next to it, such as the tool a command used.
    Neutral,
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

/// The capsule surface of a command run, in place of the gold ones.
pub fn hud_command_capsule(p: &Palette) -> Hsla {
    hud_color(p, "#EEF5FB", "#3E4A57")
}

pub fn hud_command_finish(p: &Palette) -> Hsla {
    hud_color(p, "#A3C7EAE6", "#7FAEDCE6")
}

pub fn hud_command_finish_hover(p: &Palette) -> Hsla {
    hud_color(p, "#7FAEDCF5", "#3F72ABF5")
}

pub fn hud_command_progress_fill(p: &Palette) -> Hsla {
    hud_color(p, "#A3C7EA", "#4F6480")
}

pub fn hud_command_progress_edge(p: &Palette) -> Hsla {
    hud_color(p, "#3F72AB", "#7FAEDC")
}

pub fn hud_command_lang_on(p: &Palette) -> Hsla {
    hud_color(p, "#CFE2F5", "#7FAEDC")
}

/// The Replaced outcome surface.
pub fn hud_command_replaced() -> Hsla {
    hex("#3F72AB")
}

/// Low to high exposure of the listening dots, as RGB bytes.
pub fn hud_command_dots(p: &Palette) -> [[f32; 3]; 3] {
    if p.dark {
        [[108., 134., 165.], [127., 174., 220.], [207., 226., 245.]]
    } else {
        [[52., 70., 94.], [47., 92., 140.], [63., 114., 171.]]
    }
}

/// The processing label's brightest band in a command run.
pub fn hud_command_shoulder(p: &Palette) -> Hsla {
    hud_color(p, "#706C61", "#CFE2F5")
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
    fn cached_palettes_equal_freshly_built_ones() {
        assert!(Palette::light() == Palette::build_light());
        assert!(Palette::dark() == Palette::build_dark());
    }

    #[test]
    fn system_follows_the_os_appearance_and_explicit_choice_wins() {
        assert!(Palette::resolve(Appearance::System, WindowAppearance::Dark).dark);
        assert!(!Palette::resolve(Appearance::Light, WindowAppearance::Dark).dark);
    }
}
