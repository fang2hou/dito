use gpui_kit::{Hsla, Rgba};

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
    pub warning_text: Hsla,
    pub warning_soft: Hsla,
    pub blue_text: Hsla,
    pub seg: Hsla,
    pub seg_on: Hsla,
    pub toggle_off: Hsla,
    pub knob_on: Hsla,
    pub knob_off: Hsla,
    pub input_bg: Hsla,
    pub hud_bg: Hsla,
    pub hud_border: Hsla,
    pub shadow: Hsla,
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
    pub field_bg: Hsla,
    pub field_border: Hsla,
}

impl Palette {
    pub(crate) fn light() -> Self {
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
            warning_text: hex("#8F6A1E"),
            warning_soft: hex("#F9EFD8"),
            blue_text: hex("#4674A8"),
            seg: hex("#EBE8E1"),
            seg_on: hex("#FFFFFF"),
            toggle_off: hex("#D4D1C9"),
            knob_on: hex("#FFFFFF"),
            knob_off: hex("#FFFFFF"),
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
            field_bg: hex("#FFFFFF"),
            field_border: hex("#D4D1C9"),
        }
    }

    pub(crate) fn dark() -> Self {
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
            warning_text: hex("#E5C268"),
            warning_soft: hex("#5B5340"),
            blue_text: hex("#A9C6E4"),
            seg: hex("#38352F"),
            seg_on: hex("#5A564B"),
            toggle_off: hex("#6A6455"),
            knob_on: hex("#444139"),
            knob_off: hex("#F7F5F0"),
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
            field_bg: hex("#38352F"),
            field_border: hex("#716B5E"),
        }
    }
}

pub fn hud_color(p: &Palette, light: &str, dark: &str) -> Hsla {
    hex(if p.dark { dark } else { light })
}
