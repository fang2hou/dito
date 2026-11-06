use std::time::Duration;

use gpui_kit::*;

use crate::theme::Palette;
use crate::ui::{self, hex_alpha};

/// Placeholder day groups for the first load: the toolbar is live already and only the heavy rows
/// defer. Two groups keep the shape of the real list so the swap does not reflow the page.
pub(super) fn skeleton_column(p: &Palette) -> Div {
    let mut column = div().w_full().flex().flex_col().gap(px(20.));
    for group in 0..2u8 {
        let rows = if group == 0 { 3 } else { 2 };
        let mut card = ui::card(p).flex_col().gap(px(32.)).p(px(18.));
        for r in 0..rows {
            card = card.child(skeleton_row(p, group as usize * 4 + r as usize));
        }
        column = column.child(div().w_full().flex().flex_col().gap(px(12.)).child(skeleton_heading(p)).child(card));
    }
    column
}

/// One shimmering placeholder row, sized against the real rows (~89 px pitch): two text bars
/// plus a short meta bar, action-button squares on the right, and a gloss band sweeping
/// across on a loop.
fn skeleton_row(p: &Palette, id: usize) -> Div {
    let mut bars = div().flex_1().min_w(px(0.)).flex().flex_col().gap(px(10.));
    for (w, h) in [(0.95, 16.), (0.68, 16.), (0.22, 11.)] {
        bars = bars.child(div().w(relative(w)).h(px(h)).rounded(px(h / 2.)).bg(p.seg));
    }
    let glow = hex_alpha(gpui_kit::hsla(0., 0., 1., 1.), if p.dark { 0.07 } else { 0.55 });
    let clear = gpui_kit::hsla(0., 0., 1., 0.);
    let band = div()
        .absolute()
        .top_0()
        .h_full()
        .w(relative(0.36))
        .child(div().w(relative(0.5)).h_full().bg(linear_gradient(
            90.,
            linear_color_stop(clear, 0.),
            linear_color_stop(glow, 1.),
        )))
        .child(div().w(relative(0.5)).h_full().bg(linear_gradient(
            90.,
            linear_color_stop(glow, 0.),
            linear_color_stop(clear, 1.),
        )));
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(18.))
        .relative()
        .child(bars)
        .child(
            div()
                .flex()
                .gap(px(8.))
                .child(div().size(px(28.)).rounded(px(8.)).bg(p.seg))
                .child(div().size(px(28.)).rounded_full().bg(p.seg)),
        )
        .child(div().absolute().top_0().left_0().size_full().overflow_hidden().rounded(px(10.)).child(
            band.with_animation(
                (ElementId::from(("skeleton", id)), "sweep"),
                Animation::new(Duration::from_millis(1300)).repeat(),
                |el, t| el.left(relative(t * 1.4 - 0.4)),
            ),
        ))
}

fn skeleton_heading(p: &Palette) -> Div {
    div().w(px(44.)).h(px(16.)).rounded(px(8.)).bg(p.seg)
}
