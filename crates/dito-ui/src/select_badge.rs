use gpui_kit::prelude::*;
use gpui_kit::*;

use super::callbacks::Cb;
use super::hover::{HOVER_MS, hover_tracker, is_hovered};
use super::primitives::{hex_alpha, icon};
use super::tooltip::{tip_above, tip_label};
use super::tween::tweened;
use crate::Ic;
use crate::Palette;

/// Clicking it flips its own state and leaves the menu open; it never picks the row.
pub struct SelectBadge {
    pub active: bool,
    pub tip: SharedString,
    pub on_click: Cb,
}

pub(super) fn select_badge(id: (&'static str, usize), p: &Palette, b: &SelectBadge) -> Stateful<Div> {
    let fg = match (b.active, p.dark) {
        (false, _) => hex_alpha(p.muted, 0.4),
        (true, true) => p.danger,
        (true, false) => p.danger_text,
    };
    let cb = b.on_click.clone();
    let tip_id = ElementId::from((ElementId::from(id), "badge"));
    tip_label(tip_id.clone(), b.tip.clone());
    let hover = hex_alpha(p.text, 0.08);
    // Keep the fill and glyph in one normal-flow box. Both sizes are even so centering
    // does not introduce half-pixel offsets that SVG pixel snapping rounds differently.
    div()
        .id(tip_id.clone())
        .relative()
        .size(px(24.))
        .flex_none()
        .rounded_full()
        .cursor_pointer()
        .on_hover(hover_tracker(tip_id.clone()))
        .child(tweened(
            ElementId::from((tip_id.clone(), "hover-fill")),
            if is_hovered(&tip_id) { 1. } else { 0. },
            HOVER_MS,
            move |v| {
                div()
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(Hsla { a: hover.a * v, ..hover })
                    .child(icon(Ic::Ban, 16., fg))
            },
        ))
        .child(tip_above(p, &tip_id))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, w, cx| {
            cx.stop_propagation();
            cb(w, cx);
        })
}
