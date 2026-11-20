//! Design tokens from the FigCraft design (`design:foundations/color`), light and dark.

use std::sync::LazyLock;

use dito_core::config::Appearance;
#[cfg(test)]
use gpui_kit::Rgba;
use gpui_kit::{App, Hsla, Window, WindowAppearance};

use crate::ui::{Palette as UiPalette, hex_alpha};
pub use crate::ui::{hex, hud_color};

pub const PLAIN_BLACK: &str = "#444139";

#[derive(Clone, Copy, PartialEq)]
pub struct Palette {
    pub ui: UiPalette,
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
    #[allow(dead_code, reason = "the color collection's blue pair, for app-owned status tints")]
    pub blue_soft: Hsla,
    /// Muted chips in both themes: failures keep the loud solid red, these stay quiet.
    /// Softer than `seg`, which reads too heavy on a white card.
    pub tile_idle: Hsla,
    /// Plate behind a model card while its download runs, verifies or has failed.
    pub row_idle: Hsla,
    /// Surface of the sidebar health pop-up.
    pub menu_bg: Hsla,
    pub nav_on: Hsla,
    /// Hairline that replaces section headers in the collapsed icon rail.
    pub hair: Hsla,
    /// Component tokens from the design file's `Color` collection (`button/…`, `select/…`, `model/…`).
    /// The ring that flashes around the armed stop pill (`button/danger_halo`).
    pub button_danger_halo: Hsla,
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
            ui: UiPalette::light(),
            success_text: hex("#28A788"),
            success_soft: hex("#EBF6F3"),
            success_row: hex("#EBF6F3"),
            orange: hex("#E87B2E"),
            orange_soft: hex("#FDF1E7"),
            orange_text: hex("#C25E16"),
            blue_soft: hex("#E9F0F7"),
            tile_idle: hex("#F1EFE9"),
            row_idle: hex("#FAF9F5"),
            menu_bg: hex("#FFFFFF"),
            nav_on: hex("#EBE8E1"),
            hair: hex("#D9D6CD"),
            button_danger_halo: hex("#D995888C"),
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
            ui: UiPalette::dark(),
            success_text: hex("#FFFFFF"),
            success_soft: hex("#28A788"),
            success_row: hex("#394842"),
            orange: hex("#E87B2E"),
            orange_soft: hex("#5C4330"),
            orange_text: hex("#FFB37B"),
            blue_soft: hex("#3E4A57"),
            tile_idle: hex("#38352F"),
            row_idle: hex("#423F38"),
            menu_bg: hex("#2F2C26"),
            nav_on: hex("#5A564B"),
            hair: hex("#514D43"),
            button_danger_halo: hex("#FFBAA759"),
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
}

impl Palette {
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

impl From<Palette> for UiPalette {
    fn from(palette: Palette) -> Self {
        palette.ui
    }
}

impl std::ops::Deref for Palette {
    type Target = UiPalette;

    fn deref(&self) -> &Self::Target {
        &self.ui
    }
}

impl std::ops::DerefMut for Palette {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.ui
    }
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

/// A palette that follows the system is read while a frame is built, and a change of the system appearance asks for
/// no frame by itself: without this the window keeps its old colors until something else redraws it.
pub fn repaint_with_system(window: &Window) {
    window.observe_window_appearance(|window, _| window.refresh()).detach();
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
    fn palette_colors_keep_their_original_values() {
        fn check(p: Palette, light: bool, expected: &[&str]) {
            let actual = [
                p.bg,
                p.side,
                p.card,
                p.sub,
                p.border,
                p.border_strong,
                p.text,
                p.muted,
                p.primary,
                p.on_primary,
                p.accent,
                p.accent_text,
                p.accent_soft,
                p.danger,
                p.danger_text,
                p.danger_soft,
                p.success,
                p.success_text,
                p.success_soft,
                p.success_row,
                p.orange,
                p.orange_soft,
                p.orange_text,
                p.warning_text,
                p.warning_soft,
                p.blue_text,
                p.blue_soft,
                p.seg,
                p.seg_on,
                p.tile_idle,
                p.row_idle,
                p.menu_bg,
                p.toggle_off,
                p.knob_on,
                p.knob_off,
                p.nav_on,
                p.hair,
                p.input_bg,
                p.hud_bg,
                p.hud_border,
                p.shadow,
                p.button_primary_hover,
                p.button_danger_bg,
                p.button_danger_border,
                p.button_danger_text,
                p.button_danger_hover,
                p.button_danger_halo,
                p.select_hover,
                p.select_hover_border,
                p.segment_hover,
                p.segment_hover_label,
                p.switch_knob_shadow,
                p.slider_thumb_hover,
                p.slider_thumb_shadow,
                p.slider_thumb_shadow_pressed,
                p.model_meta,
                p.model_download_bg,
                p.model_download_border,
                p.model_idle_bg,
                p.model_idle_border,
                p.model_idle_border_hover,
                p.model_badge_text,
                p.model_error_text,
                p.field_bg,
                p.field_border,
                p.command_primary,
                p.command_primary_hover,
                p.command_soft,
                p.command_soft_hover,
                p.command_glow,
                p.stats_heat_0,
                p.stats_heat_1,
                p.stats_heat_2,
                p.stats_heat_3,
                p.stats_heat_4,
            ];
            assert_eq!(p.dark, !light);
            assert_eq!(actual.len(), expected.len());
            for (actual, expected) in actual.into_iter().zip(expected) {
                assert_eq!(actual, hex(expected), "{expected}");
            }
        }

        check(
            Palette::light(),
            true,
            &[
                "#FFFFFF",
                "#F7F5F0",
                "#FFFFFF",
                "#F7F5F0",
                "#EBE8E1",
                "#D4D1C9",
                "#444139",
                "#6A6455",
                "#AA8C40",
                "#FFFFFF",
                "#AA8C40",
                "#826F41",
                "#FFF9EB",
                "#FF4949",
                "#FF4949",
                "#FFEFEF",
                "#28A788",
                "#28A788",
                "#EBF6F3",
                "#EBF6F3",
                "#E87B2E",
                "#FDF1E7",
                "#C25E16",
                "#8F6A1E",
                "#F9EFD8",
                "#4674A8",
                "#E9F0F7",
                "#EBE8E1",
                "#FFFFFF",
                "#F1EFE9",
                "#FAF9F5",
                "#FFFFFF",
                "#D4D1C9",
                "#FFFFFF",
                "#FFFFFF",
                "#EBE8E1",
                "#D9D6CD",
                "#FFFFFF",
                "#FFFFFF",
                "#D4D1C9",
                "#4441391F",
                "#FFFFFF29",
                "#FFF0EC",
                "#D99588",
                "#A44232",
                "#A4423224",
                "#D995888C",
                "#EBE8E18C",
                "#6A645580",
                "#FFFFFF8C",
                "#534F44",
                "#0000000D",
                "#F0EEE9",
                "#0000002E",
                "#00000066",
                "#6A6455",
                "#FAF9F5",
                "#EBE8E1",
                "#F7F5F0",
                "#D4D1C9",
                "#B8AE9C",
                "#6A6455",
                "#444139",
                "#FFFFFF",
                "#D4D1C9",
                "#3F72AB",
                "#5D90C4",
                "#EEF5FB",
                "#CFE2F5",
                "#3F72AB73",
                "#EBE8E1",
                "#FFECBD",
                "#F6DA97",
                "#C2A765",
                "#AA8C40",
            ],
        );
        check(
            Palette::dark(),
            false,
            &[
                "#444139",
                "#38352F",
                "#4D493F",
                "#3E3B34",
                "#5A564B",
                "#6A6455",
                "#F7F5F0",
                "#C2BDB2",
                "#E0C47D",
                "#444139",
                "#E0C47D",
                "#E0C47D",
                "#5B5340",
                "#FF4949",
                "#FFFFFF",
                "#FF4949",
                "#28A788",
                "#FFFFFF",
                "#28A788",
                "#394842",
                "#E87B2E",
                "#5C4330",
                "#FFB37B",
                "#E5C268",
                "#5B5340",
                "#A9C6E4",
                "#3E4A57",
                "#38352F",
                "#5A564B",
                "#38352F",
                "#423F38",
                "#2F2C26",
                "#6A6455",
                "#444139",
                "#F7F5F0",
                "#5A564B",
                "#514D43",
                "#3E3B34",
                "#444139",
                "#6A6455",
                "#00000066",
                "#00000029",
                "#5A3831",
                "#A76C5C",
                "#FFBAA7",
                "#FFBAA738",
                "#FFBAA759",
                "#38352F8C",
                "#C2BDB280",
                "#5A564B8C",
                "#E2DFD7",
                "#00000033",
                "#524F46",
                "#0000002E",
                "#00000066",
                "#C9C3B7",
                "#423F38",
                "#5A564B",
                "#423F38",
                "#716B5E",
                "#918775",
                "#C9C3B7",
                "#F7F5F0",
                "#38352F",
                "#716B5E",
                "#7FAEDC",
                "#A3C7EA",
                "#3E4A57",
                "#4A5B70",
                "#7FAEDC8C",
                "#3E3B34",
                "#685218",
                "#826F41",
                "#AA8C40",
                "#E0C47D",
            ],
        );
    }

    #[test]
    fn system_follows_the_os_appearance_and_explicit_choice_wins() {
        assert!(Palette::resolve(Appearance::System, WindowAppearance::Dark).dark);
        assert!(!Palette::resolve(Appearance::Light, WindowAppearance::Dark).dark);
    }
}
