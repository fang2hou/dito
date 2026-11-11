use gpui_kit::*;

use crate::ui::hex_alpha;

#[derive(Clone, Copy)]
pub enum Edge {
    Top,
    Bottom,
}

/// `right_inset` keeps it clear of a scrollbar.
pub fn edge_fade(edge: Edge, color: Hsla, height: f32, right_inset: f32) -> Div {
    let strip = div().absolute().left_0().right(px(right_inset)).h(px(height));
    let (strip, angle) = match edge {
        Edge::Top => (strip.top_0(), 180.),
        Edge::Bottom => (strip.bottom_0(), 0.),
    };
    strip.bg(linear_gradient(angle, linear_color_stop(color, 0.), linear_color_stop(hex_alpha(color, 0.), 1.)))
}
