//! The word chip of `design:settings/wordbook` and its selection marker, shared by every list of chips to choose
//! from.

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::hover::hover_tracker;
use super::primitives::{icon, mix_rgb};
use super::typography::text;
use crate::assets::Ic;
use crate::theme::Palette;

/// The 16 pt selection marker of a word chip: `hover` fills the inside of its outline, `chosen` turns it into the
/// solid disc with a check.
pub fn selection_ring(p: &Palette, id: ElementId, hover: f32, chosen: f32) -> Stateful<Div> {
    let hover_fill = Hsla { a: hover, ..p.seg };
    div()
        .id(id.clone())
        .size(px(16.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(Hsla { a: p.muted.a * (1. - chosen), ..p.muted })
        .bg(mix_rgb(hover_fill, p.primary, chosen))
        .on_hover(hover_tracker(id))
        .when(chosen > 0., |el| el.child(div().opacity(chosen).child(icon(Ic::Check, 12., p.on_primary))))
}

/// A word chip without its marker; `chosen` blends it to the selected surface and border.
pub fn chip_surface(p: &Palette, id: ElementId, word: SharedString, chosen: f32) -> Stateful<Div> {
    div()
        .id(id.clone())
        .on_hover(hover_tracker(id))
        .min_w(px(0.))
        .h(px(44.))
        .pl(px(16.))
        .pr(px(10.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .rounded_full()
        .bg(mix_rgb(p.card, p.accent_soft, chosen))
        .border_1()
        .border_color(mix_rgb(p.border, p.accent, chosen))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .overflow_hidden()
                .child(text(word, 14., FontWeight::MEDIUM, p.text).whitespace_nowrap().text_ellipsis()),
        )
}
