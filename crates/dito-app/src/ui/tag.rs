use gpui_kit::{Div, SharedString};

use crate::theme::{Palette, Tint};

pub fn tag(p: &Palette, label: impl Into<SharedString>, tint: Tint) -> Div {
    let (bg, fg) = p.on_tint(tint);
    super::layout::tinted_tag(label, bg, fg)
}
