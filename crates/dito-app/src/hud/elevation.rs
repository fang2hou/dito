//! The one subtle elevation layer of every HUD surface.

use gpui_kit::*;

use crate::theme::{Palette, hex};
use crate::ui::hex_alpha;

use super::Scale;

/// What a shadow lifts: a surface (capsule, pill, bubble) or a round button on one.
#[derive(Clone, Copy)]
pub(super) enum Elevation {
    Surface,
    Button,
}

/// One subtle elevation layer, kept inside the transparent window's reserved padding.
pub(super) fn surface_shadow(p: &Palette, scale: Scale, elevation: Elevation) -> Vec<BoxShadow> {
    let (y, blur, opacity) = match elevation {
        Elevation::Button => (1., 2., if p.dark { 0.14 } else { 0.08 }),
        Elevation::Surface => (1., 4., if p.dark { 0.24 } else { 0.12 }),
    };
    vec![BoxShadow {
        color: hex_alpha(hex("#000000"), opacity),
        offset: point(scale.px(0.), scale.px(y)),
        blur_radius: scale.px(blur),
        spread_radius: scale.px(0.),
        inset: false,
    }]
}
