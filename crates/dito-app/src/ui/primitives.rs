use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::{HudIc, Ic};

pub fn icon(ic: Ic, size: f32, color: Hsla) -> Svg {
    svg().path(ic.path()).size(px(size)).flex_none().text_color(color)
}

pub fn hud_icon(ic: HudIc, size: f32, color: Hsla) -> Svg {
    svg().path(ic.path()).size(px(size)).flex_none().text_color(color)
}

pub fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let l = |x: f32, y: f32| x + (y - x) * t;
    Hsla { h: l(a.h, b.h), s: l(a.s, b.s), l: l(a.l, b.l), a: l(a.a, b.a) }
}

/// For colors too far apart in hue for the HSL blend of [`mix`].
pub fn mix_rgb(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b): (Rgba, Rgba) = (a.into(), b.into());
    Rgba { r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t, a: a.a + (b.a - a.a) * t }
        .into()
}

pub fn white() -> Hsla {
    Hsla { h: 0., s: 0., l: 1., a: 1. }
}

pub fn transparent() -> Hsla {
    Hsla { h: 0., s: 0., l: 0., a: 0. }
}

pub fn hex_alpha(c: Hsla, a: f32) -> Hsla {
    Hsla { a, ..c }
}
