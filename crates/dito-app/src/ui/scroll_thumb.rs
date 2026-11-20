//! The overlay scroll thumb of `design:tools/transcripts`: 4 pt wide in border_strong, 6 pt beside the scrolled
//! content, shown only while the content overflows and the pointer is over it.

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::hover::HOVER_MS;
use super::tween::tweened;
use crate::ui::Palette;

const THUMB_W: f32 = 4.;
const THUMB_GAP: f32 = 6.;
const THUMB_MIN: f32 = 32.;

/// Where the content stands: the viewport height, how far it can scroll, and how far it has.
#[derive(Clone, Copy)]
pub struct ScrollExtent {
    pub viewport: f32,
    pub max: f32,
    pub scrolled: f32,
}

/// The thumb for a positioned parent that holds the scrolled content. `reserve` keeps the track clear of something
/// floating over the bottom of the content.
pub fn overlay_thumb(
    id: impl Into<ElementId>,
    p: &Palette,
    hovered: bool,
    extent: ScrollExtent,
    reserve: f32,
) -> AnyElement {
    let ScrollExtent { viewport, max, scrolled } = extent;
    if viewport <= 0. || max <= 0.5 {
        return div().into_any_element();
    }
    let scrolled = scrolled.clamp(0., max);
    let track = (viewport - reserve).max(THUMB_MIN);
    let length = (track * viewport / (viewport + max)).max(THUMB_MIN).min(track);
    let top = (track - length) * scrolled / max;
    let color = p.border_strong;
    tweened(id.into(), f32::from(hovered), HOVER_MS, move |opacity| {
        div()
            .absolute()
            .top(px(top))
            .right(px(-(THUMB_GAP + THUMB_W)))
            .w(px(THUMB_W))
            .h(px(length))
            .rounded(px(THUMB_W / 2.))
            .bg(color.opacity(opacity))
    })
    .into_any_element()
}
