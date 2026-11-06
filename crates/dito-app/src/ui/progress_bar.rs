use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::Palette;

pub fn progress_bar(p: &Palette, fraction: f32) -> Div {
    div()
        .w_full()
        .h(px(8.))
        .rounded(px(4.))
        .bg(p.seg)
        .child(div().h_full().w(relative(fraction.clamp(0., 1.))).rounded(px(4.)).bg(p.accent))
}
