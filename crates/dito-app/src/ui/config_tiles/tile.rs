//! One tile of a configuration row (`design:settings/cloud-models/tile-row`).

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::ConfigTile;
use super::geometry::TILE_H;
use crate::ui::Palette;
use crate::ui::{hex_alpha, hover_fill_if, hover_tracker, radio, ring_overlay, text, transparent};

/// Hovers like the choice tiles of the Models page. `hover` turns the fill off while another tile is dragged.
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
        .gap(px(12.))
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

fn line(s: SharedString, size: f32, weight: FontWeight, color: Hsla, line_height: f32) -> Div {
    text(s, size, weight, color).h(px(line_height)).line_height(px(line_height)).truncate()
}
