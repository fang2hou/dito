//! The round buttons a capsule carries: their look, hover cross-fade and static rendering.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::HudIc;
use crate::theme::{self, PLAIN_BLACK, Palette, hex};
use crate::ui::{self, hex_alpha, hud_icon};

use super::Scale;
use super::elevation::{self, Elevation};

/// Round button inside the capsule.
pub(super) const BTN: f32 = 23.0;
/// Default recovery-action glyph box in design points (rendered via `tx`).
const BTN_ICON: f32 = 13.;

/// Look of one round HUD button. `size` is in design points and rendered via [`Scale::tx`].
#[derive(Clone, Copy)]
pub(super) struct Circle {
    pub(super) id: &'static str,
    pub(super) bg: Hsla,
    /// Background while hovered. Hover never lightens: the gold button deepens a step, the danger
    /// buttons swap to their inverted scheme.
    pub(super) hover_bg: Hsla,
    glyph: HudIc,
    size: f32,
    color: Hsla,
    /// Glyph color while hovered (the danger buttons invert their glyph with the fill).
    pub(super) hover_color: Hsla,
    /// Resting hairline: `(color, opacity)`, painted inside the button and fading out on hover.
    pub(super) ring: Option<(Hsla, f32)>,
    dark: bool,
}

/// The round buttons a capsule can carry.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Btn {
    Cancel,
    Finish,
    Dismiss,
    Retry,
    Settings,
}

impl Btn {
    pub(super) fn look(self, p: &Palette) -> Circle {
        let (glass, glass_hover) =
            (theme::hud_color(p, "#EBE8E1", "#5A564B"), theme::hud_color(p, "#D4D1C9", "#6A6455"));
        let red = hex("#FF4949");
        // Danger buttons invert on hover in both themes.
        let danger = || {
            (
                theme::hud_color(p, "#FFFFFF", "#FF4949"),
                theme::hud_color(p, "#FF4949", "#FFFFFF"),
                theme::hud_color(p, "#FF4949", "#FFEFEF"),
                theme::hud_color(p, "#FFFFFF", "#FF4949"),
                (!p.dark).then_some((red, 0.25)),
            )
        };
        let (id, bg, hover_bg, glyph, size, color, hover_color, ring) = match self {
            Self::Cancel => ("hud-cancel", glass, glass_hover, HudIc::X, 15., p.text, p.text, None),
            Self::Finish => (
                "hud-finish",
                theme::hud_color(p, "#F6DA97", "#E0C47D"),
                theme::hud_color(p, "#E0C47D", "#AA8C40"),
                HudIc::Finish,
                14.,
                hex(PLAIN_BLACK),
                hex(PLAIN_BLACK),
                None,
            ),
            Self::Dismiss => {
                let (bg, hbg, c, hc, ring) = danger();
                ("hud-dismiss", bg, hbg, HudIc::X, 15., c, hc, ring)
            }
            Self::Retry => {
                let (bg, hbg, c, hc, ring) = danger();
                ("hud-retry", bg, hbg, HudIc::Retry, BTN_ICON, c, hc, ring)
            }
            Self::Settings => {
                let (bg, hbg, c, hc, ring) = danger();
                ("hud-settings", bg, hbg, HudIc::Sliders, BTN_ICON, c, hc, ring)
            }
        };
        Circle { id, bg, hover_bg, glyph, size, color, hover_color, ring, dark: p.dark }
    }
}

/// A round button's shell: `bg` is its background layer, `glyph` sits on top (already colored —
/// see [`hover_glyph`] for the hover cross-fade version).
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

/// The background layer at cross-fade position `t` (0 resting, 1 hovered): the fill blends and a
/// resting hairline, if any, fades out with it.
pub(super) fn hover_layer(c: &Circle, scale: Scale, t: f32) -> AnyElement {
    let mut layer = div().absolute().inset_0().rounded_full().bg(ui::mix_rgb(c.bg, c.hover_bg, t));
    if let Some((color, alpha)) = c.ring {
        layer = layer.border(scale.px(0.75)).border_color(hex_alpha(color, alpha * (1. - t)));
    }
    layer.into_any_element()
}

/// The glyph at cross-fade position `t`: danger buttons invert their glyph with the fill, so both
/// copies are stacked and blended by opacity.
pub(super) fn hover_glyph(c: &Circle, scale: Scale, t: f32) -> AnyElement {
    let base = hud_icon(c.glyph, scale.tx(c.size), c.color);
    if c.color == c.hover_color {
        return base.into_any_element();
    }
    div()
        .relative()
        .size(gpui_kit::px(scale.tx(c.size)))
        .child(base.opacity(1. - t))
        .child(hud_icon(c.glyph, scale.tx(c.size), c.hover_color).opacity(t).absolute().inset_0())
        .into_any_element()
}

/// A button drawn as the HUD draws it, without the behaviour: for capsules that are only shown.
pub(crate) fn static_button(b: Btn, scale: Scale, p: &Palette) -> Div {
    let look = b.look(p);
    let Circle { id: _, bg, hover_bg: _, glyph: _, size: _, color: _, hover_color: _, ring, dark: _ } = &look;
    let mut layer = div().absolute().inset_0().rounded_full().bg(*bg);
    if let Some((color, alpha)) = ring {
        layer = layer.border(scale.px(0.75)).border_color(hex_alpha(*color, *alpha));
    }
    circle_shell(
        &look,
        scale,
        layer.into_any_element(),
        hud_icon(look.glyph, scale.tx(look.size), look.color).into_any_element(),
    )
}

/// The legend uses exactly the same button colors as its HUD preview.
pub(crate) fn legend_button(b: Btn, p: &Palette) -> Div {
    static_button(b, Scale::DESIGN, p)
}
