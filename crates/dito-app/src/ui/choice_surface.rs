//! The card of a choice the user makes by picking one of a few, as the onboarding steps draw it (`design:onboarding`).

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::hover::{hover_fill_if, hover_tracker};
use super::inputs::with_ring;
use super::primitives::hex_alpha;
use crate::theme::Palette;

const RADIUS: f32 = 16.;

/// The surface alone: the caller adds the size, padding, cursor and click, then the content, which paints above the
/// hover fill and the ring. The selected card keeps its own surface under the pointer.
pub fn choice_surface(p: &Palette, id: impl Into<ElementId>, on: bool) -> Stateful<Div> {
    let id: ElementId = id.into();
    let surface = div()
        .id(id.clone())
        .rounded(px(RADIUS))
        .bg(if on { p.accent_soft } else { p.card })
        .on_hover(hover_tracker(id.clone()))
        .child(hover_fill_if(&id, Some(RADIUS), hex_alpha(p.seg, 0.6), !on));
    with_ring(surface, (id, "ring"), on, RADIUS, p.border, p.accent)
}
