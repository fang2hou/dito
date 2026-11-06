use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::callbacks::Cb;
use super::hover::{hover_fill, hover_tracker};
use super::tween::RING_MS;
use crate::theme::Palette;

/// The bordered card that holds menu rows; it closes itself on any press outside it.
pub fn menu_panel(id: ElementId, p: &Palette, on_close: Cb) -> Stateful<Div> {
    div()
        .id(id)
        .p(px(6.))
        .flex()
        .flex_col()
        .gap(px(1.))
        .rounded(px(12.))
        .bg(p.card)
        .border_1()
        .border_color(p.border)
        .shadow(vec![BoxShadow {
            color: p.shadow,
            offset: point(px(0.), px(10.)),
            blur_radius: px(28.),
            spread_radius: px(0.),
            inset: false,
        }])
        .occlude()
        .on_mouse_down_out(move |_, w, cx| on_close(w, cx))
}

/// A hoverable row of a menu panel; the caller adds its click handler and content.
pub fn menu_row(id: ElementId, p: &Palette) -> Stateful<Div> {
    div()
        .id(id.clone())
        .relative()
        .h(px(34.))
        .px(px(12.))
        .flex()
        .items_center()
        .gap(px(10.))
        .rounded(px(8.))
        .cursor_pointer()
        .on_hover(hover_tracker(id.clone()))
        .child(hover_fill(&id, Some(8.), p.seg))
}

/// Places `panel` `top` px under its relative parent with the right edges aligned. It is drawn in the
/// deferred layer: above siblings and not clipped by scroll or overflow containers; it only snaps to the
/// window edges.
pub fn float<E: IntoElement + Styled + 'static>(id: ElementId, top: f32, panel: E) -> Div {
    // Fades in and settles 6px upwards on the same clock and easing as the focus ring.
    let appear = panel.with_animation((id, "appear"), Animation::new(Duration::from_millis(RING_MS)), |p, t| {
        let q = t * t * (3. - 2. * t);
        p.opacity(q).relative().top(px((1. - q) * -6.))
    });
    div().absolute().top(px(top)).right(px(0.)).size(px(0.)).child(
        deferred(anchored().anchor(Anchor::TopRight).snap_to_window_with_margin(px(8.)).child(appear)).with_priority(1),
    )
}
