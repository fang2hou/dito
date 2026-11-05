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

pub(super) fn hex_black(a: f32) -> Hsla {
    Hsla { h: 0., s: 0., l: 0., a }
}

pub fn white() -> Hsla {
    hex_white()
}

pub(super) fn hex_white() -> Hsla {
    Hsla { h: 0., s: 0., l: 1., a: 1. }
}

pub fn transparent() -> Hsla {
    Hsla { h: 0., s: 0., l: 0., a: 0. }
}

/// `c` with its alpha replaced.
pub fn hex_alpha(c: Hsla, a: f32) -> Hsla {
    Hsla { a, ..c }
}
