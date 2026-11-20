use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::Palette;

pub fn radio(p: &Palette, on: bool) -> Div {
    let d = div().size(px(18.)).flex_none().rounded_full().bg(p.input_bg);
    if on { d.border(px(6.)).border_color(p.primary) } else { d.border(px(1.5)).border_color(p.border_strong) }
}
