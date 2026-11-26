//! The word chip of the wordbook and its selection marker (`design:settings/wordbook/find-words`,
//! `design:settings/wordbook/spoken-forms`), shared by every list of chips to choose from.

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::hover::hover_tracker;
use super::primitives::{icon, mix_rgb};
use super::typography::text;
use crate::Ic;
use crate::Palette;

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
    chip(p, id, 44., chosen).gap(px(8.)).child(one_line(word, 14., p.text))
}

/// The word chip of the wordbook grid, 48 pt whether it shows its spoken forms or not, so the grid keeps one row
/// pitch. Without `spoken` the word is one centered line as in [`chip_surface`]; with it, the word and its muted
/// language name, when it has one, sit above one truncated line of the spoken forms.
pub fn wordbook_chip_surface(
    p: &Palette,
    id: ElementId,
    word: SharedString,
    lang: Option<SharedString>,
    spoken: Option<SharedString>,
    chosen: f32,
) -> Stateful<Div> {
    let chip = chip(p, id, 48., chosen);
    let Some(spoken) = spoken else {
        return chip.gap(px(8.)).child(one_line(word, 14., p.text));
    };
    let muted = |s: SharedString| text(s, 11., FontWeight::NORMAL, p.muted).line_height(px(15.));
    chip.gap(px(6.)).child(
        div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        text(word, 13.5, FontWeight::MEDIUM, p.text)
                            .line_height(px(18.))
                            .truncate()
                            .flex_1()
                            .min_w(px(0.)),
                    )
                    .children(lang.map(|lang| muted(lang).flex_none())),
            )
            .child(muted(spoken).w_full().min_w(px(0.)).truncate()),
    )
}

fn chip(p: &Palette, id: ElementId, height: f32, chosen: f32) -> Stateful<Div> {
    div()
        .id(id.clone())
        .on_hover(hover_tracker(id))
        .min_w(px(0.))
        .h(px(height))
        .pl(px(16.))
        .pr(px(10.))
        .flex()
        .items_center()
        .justify_between()
        .rounded_full()
        .bg(mix_rgb(p.card, p.accent_soft, chosen))
        .border_1()
        .border_color(mix_rgb(p.border, p.accent, chosen))
}

/// The word on one line, which truncates when the chip is too narrow.
fn one_line(word: SharedString, size: f32, color: Hsla) -> Div {
    div()
        .flex_1()
        .min_w(px(0.))
        .overflow_hidden()
        .child(text(word, size, FontWeight::MEDIUM, color).whitespace_nowrap().text_ellipsis())
}
