//! The round buttons a capsule carries.

use dito_core::i18n::{Lang, Tr};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::Ic;
use crate::theme::{self, PLAIN_BLACK, Palette, hex};
use crate::ui::{self, hex_alpha, icon};

use super::Scale;
use super::elevation::{self, Elevation};

pub(super) const BTN: f32 = crate::ui::CircleSize::Compact.dimensions().0;
/// A shared 24 pt vector canvas rendered in a 14 pt slot (`design:hud/buttons`).
pub(super) const BTN_ICON: f32 = crate::ui::CircleSize::Compact.dimensions().1;

#[derive(Clone, Copy)]
pub(super) struct Circle {
    pub(super) id: &'static str,
    pub(super) bg: Hsla,
    /// Hover never lightens: the gold button deepens a step, the danger buttons swap to their inverted scheme.
    pub(super) hover_bg: Hsla,
    glyph: Ic,
    color: Hsla,
    pub(super) hover_color: Hsla,
    /// Resting hairline as `(color, opacity)`.
    pub(super) ring: Option<(Hsla, f32)>,
    dark: bool,
    frosted: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Btn {
    Cancel,
    Finish,
    Dismiss,
    Retry,
    Settings,
}

impl Btn {
    /// `command`: the run was started by the command shortcut, which swaps gold for the ocean ramp.
    pub(super) fn look(self, p: &Palette, command: bool) -> Circle {
        let (glass, glass_hover) =
            (theme::hud_color(p, "#EBE8E1", "#5A564B"), theme::hud_color(p, "#D4D1C9", "#6A6455"));
        let red = hex("#FF4949");
        let danger = || {
            (
                theme::hud_color(p, "#FFFFFF", "#FF4949"),
                theme::hud_color(p, "#FF4949", "#FFFFFF"),
                theme::hud_color(p, "#FF4949", "#FFEFEF"),
                theme::hud_color(p, "#FFFFFF", "#FF4949"),
                (!p.dark).then_some((red, 0.25)),
            )
        };
        let (id, bg, hover_bg, glyph, color, hover_color, ring) = match self {
            Self::Cancel => ("hud-cancel", glass, glass_hover, Ic::X, p.text, p.text, None),
            Self::Finish => (
                "hud-finish",
                if command { theme::hud_command_finish(p) } else { theme::hud_color(p, "#F6DA97", "#E0C47D") },
                if command { theme::hud_command_finish_hover(p) } else { theme::hud_color(p, "#E0C47D", "#AA8C40") },
                Ic::Check,
                hex(PLAIN_BLACK),
                hex(PLAIN_BLACK),
                None,
            ),
            Self::Dismiss => {
                let (bg, hbg, c, hc, ring) = danger();
                ("hud-dismiss", bg, hbg, Ic::X, c, hc, ring)
            }
            Self::Retry => {
                let (bg, hbg, c, hc, ring) = danger();
                ("hud-retry", bg, hbg, Ic::Refresh, c, hc, ring)
            }
            Self::Settings => {
                let (bg, hbg, c, hc, ring) = danger();
                ("hud-settings", bg, hbg, Ic::Sliders, c, hc, ring)
            }
        };
        let frosted = matches!(self, Self::Cancel | Self::Finish);
        Circle { id, bg, hover_bg, glyph, color, hover_color, ring, dark: p.dark, frosted }
    }
}

pub(super) fn circle_shell(c: &Circle, scale: Scale, bg: AnyElement, glyph: AnyElement) -> Div {
    div()
        .size(scale.px(BTN))
        .flex_none()
        .relative()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .shadow(elevation::surface_shadow(
            &if c.dark { Palette::dark() } else { Palette::light() },
            scale,
            Elevation::Button,
        ))
        .child(bg)
        .child(glyph)
}

/// `t` is the cross-fade position: 0 resting, 1 hovered.
pub(super) fn hover_layer(c: &Circle, scale: Scale, t: f32) -> AnyElement {
    let mut tint = ui::mix_rgb(c.bg, c.hover_bg, t);
    if c.frosted {
        tint.a = 0.96 + (1. - 0.96) * t;
    }
    let mut layer = div().absolute().inset_0().rounded_full().bg(tint);
    if c.frosted {
        let edge = if c.dark { hex("#F7F5F0") } else { hex("#665633") };
        layer = layer.border(scale.px(0.75)).border_color(hex_alpha(edge, if c.dark { 0.24 } else { 0.20 }));
    }
    if let Some((color, alpha)) = c.ring {
        layer = layer.border(scale.px(0.75)).border_color(hex_alpha(color, alpha * (1. - t)));
    }
    layer.into_any_element()
}

/// Danger buttons invert their glyph with the fill, so both copies are stacked and blended by opacity.
pub(super) fn hover_glyph(c: &Circle, scale: Scale, t: f32) -> AnyElement {
    let base = icon(c.glyph, scale.tx(BTN_ICON), c.color);
    if c.color == c.hover_color {
        return base.into_any_element();
    }
    div()
        .relative()
        .size(gpui_kit::px(scale.tx(BTN_ICON)))
        .child(base.opacity(1. - t))
        .child(icon(c.glyph, scale.tx(BTN_ICON), c.hover_color).opacity(t).absolute().inset_0())
        .into_any_element()
}

pub(crate) fn static_button(b: Btn, scale: Scale, p: &Palette, command: bool) -> Div {
    let look = b.look(p, command);
    circle_shell(
        &look,
        scale,
        hover_layer(&look, scale, 0.),
        icon(look.glyph, scale.tx(BTN_ICON), look.color).into_any_element(),
    )
}

/// Uses the HUD's button colors so the legend matches its preview.
pub(crate) fn legend_button(b: Btn, p: &Palette) -> Div {
    static_button(b, Scale::DESIGN, p, false)
}

pub(super) fn stop_width(lang: Lang) -> f32 {
    2. * 10.75 + BTN_ICON + 5. + ui::text_width(lang.tr(Tr::HudStop), 12., FontWeight::NORMAL)
}

pub(super) fn stop_pill(p: &Palette, lang: Lang, hover: f32) -> Div {
    let ink = theme::hud_stop_ink(p);
    let foreground = ui::mix_rgb(ink, theme::hud_color(p, "#FFFFFF", "#24221D"), hover);
    div()
        .w(Scale::HUD.px(stop_width(lang)))
        .h(Scale::HUD.px(29.))
        .rounded_full()
        .bg(ui::mix_rgb(theme::hud_error_bg(p), ink, hover))
        .border(Scale::HUD.px(0.75))
        .border_color(theme::hud_error_pill_border())
        .shadow(elevation::surface_shadow(p, Scale::HUD, Elevation::Surface))
        .flex()
        .items_center()
        .justify_center()
        .gap(Scale::HUD.px(5.))
        .child(icon(Ic::Stop, Scale::HUD.tx(BTN_ICON), foreground))
        .child(
            ui::text(lang.tr(Tr::HudStop), Scale::HUD.tx(12.), FontWeight::NORMAL, foreground)
                .line_height(Scale::HUD.px(18.)),
        )
}
