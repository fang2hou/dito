//! One tile of a configuration row (`20.05.13`): the name over the detail, the radio on the right.

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::ConfigTile;
use super::geometry::TILE_H;
use crate::theme::Palette;
use crate::ui::{hex_alpha, hover_fill_if, hover_tracker, radio, ring_overlay, text, transparent};

/// The tile as the row draws it. It hovers like the choice tiles of the Models page: a quiet fill fades in on an idle
/// one, the border grows and turns to the accent when it becomes the selected one. `hover` turns the fill off while
/// another tile is dragged. The caller sizes and places it.
pub(super) fn tile(p: &Palette, id: ElementId, spec: &ConfigTile, hover: bool) -> Stateful<Div> {
    let on = spec.selected;
    let text_block = div()
        .flex_1()
        .min_w(px(0.))
        .flex()
        .flex_col()
        .gap(px(1.))
        .child(line(spec.name.clone(), 14., FontWeight::MEDIUM, p.text, 21.))
        .child(line(spec.detail.clone(), 12., FontWeight::NORMAL, p.muted, 18.));
    div()
        .id(id.clone())
        .relative()
        .h(px(TILE_H))
        .px(px(16.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .rounded(px(14.))
        .cursor_pointer()
        .bg(if on { p.accent_soft } else { p.card })
        .border_1()
        .border_color(transparent())
        .on_hover(hover_tracker(id.clone()))
        .child(hover_fill_if(&id, Some(14.), hex_alpha(p.seg, 0.6), !on && hover))
        .child(ring_overlay((id, "ring"), on, 14., p.border_strong, p.accent))
        .child(text_block)
        .child(radio(p, on))
}

/// One line of a tile: a 21 pt box that cuts its text with an ellipsis.
fn line(s: SharedString, size: f32, weight: FontWeight, color: Hsla, line_height: f32) -> Div {
    text(s, size, weight, color).h(px(21.)).line_height(px(line_height)).truncate()
}
