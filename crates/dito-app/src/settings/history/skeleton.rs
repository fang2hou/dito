use std::time::Duration;

use gpui_kit::*;

use crate::theme::Palette;
use crate::ui::{self, Motion, hex_alpha};

/// Two groups keep the shape of the real list so the swap does not reflow the page.
pub(super) fn skeleton_column(p: &Palette) -> Div {
    let mut column = div().w_full().flex().flex_col().gap(px(20.));
    let animate = !Motion::system().is_reduced();
    for group in 0..2u8 {
        let rows = if group == 0 { 3 } else { 2 };
        let mut card = ui::card(p).flex_col().gap(px(32.)).p(px(18.));
        for r in 0..rows {
            card = card.child(skeleton_row(p, group as usize * 4 + r as usize, animate));
        }
        column = column.child(div().w_full().flex().flex_col().gap(px(12.)).child(skeleton_heading(p)).child(card));
    }
    column
}

/// Sized against the real rows (~89 px pitch).
fn skeleton_row(p: &Palette, id: usize, animate: bool) -> Div {
    let mut bars = div().flex_1().min_w(px(0.)).flex().flex_col().gap(px(10.));
    let seg = p.seg;
    let glow = hex_alpha(gpui_kit::hsla(0., 0., 1., 1.), if p.dark { 0.07 } else { 0.55 });
    let clear = hex_alpha(seg, 0.);
    for (line, (w, h)) in [(0.95, 16.), (0.68, 16.), (0.22, 11.)].into_iter().enumerate() {
        let mut bar = div().relative().w(relative(w)).h(px(h)).rounded(px(h / 2.)).bg(seg);
        if animate {
            // GPUI's overflow clip is rectangular, so the moving band must not be a child behind
            // the bar. Both full-size overlays paint their own rounded quads instead.
            let layer = || div().absolute().top_0().left_0().size_full().rounded(px(h / 2.));
            bar = bar
                .child(layer().with_animation(
                    (ElementId::from(("skeleton", id)), ["rise-1", "rise-2", "rise-3"][line]),
                    Animation::new(Duration::from_millis(1300)).repeat(),
                    move |el, t| {
                        let start = (t * 1.4 - 0.4) / w;
                        el.bg(linear_gradient(
                            90.,
                            linear_color_stop(clear, start),
                            linear_color_stop(glow, start + 0.18 / w),
                        ))
                    },
                ))
                .child(layer().with_animation(
                    (ElementId::from(("skeleton", id)), ["fall-1", "fall-2", "fall-3"][line]),
                    Animation::new(Duration::from_millis(1300)).repeat(),
                    move |el, t| {
                        let middle = (t * 1.4 - 0.22) / w;
                        el.bg(linear_gradient(
                            90.,
                            linear_color_stop(clear, middle),
                            linear_color_stop(seg, middle + 0.18 / w),
                        ))
                    },
                ));
        }
        bars = bars.child(bar);
    }
    div().w_full().flex().items_center().gap(px(18.)).child(bars).child(
        div()
            .flex()
            .gap(px(8.))
            .child(div().size(px(28.)).rounded(px(8.)).bg(p.seg))
            .child(div().size(px(28.)).rounded_full().bg(p.seg)),
    )
}

fn skeleton_heading(p: &Palette) -> Div {
    div().w(px(44.)).h(px(16.)).rounded(px(8.)).bg(p.seg)
}
