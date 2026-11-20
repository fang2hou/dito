//! The Method Label of `design:components` (11.03): a solid pill that names an HTTP method or a kind of MCP item.

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::typography::family_for;
use crate::ui::Palette;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MethodKind {
    Get,
    Post,
    /// A kind word such as a tool or a resource, in the neutral colors of the Neutral Status Label.
    Neutral,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MethodSize {
    /// A list row: 20 pt high.
    Row,
    /// The leading slot of a dialog title: 24 pt high.
    Title,
}

impl MethodSize {
    const fn dimensions(self) -> (f32, f32) {
        match self {
            Self::Row => (20., 7.),
            Self::Title => (24., 9.),
        }
    }
}

pub fn method_label(p: &Palette, kind: MethodKind, label: impl Into<SharedString>, size: MethodSize) -> Div {
    let label: SharedString = label.into();
    let (bg, fg) = match kind {
        MethodKind::Get => (p.blue_text, p.on_primary),
        MethodKind::Post => (p.accent, p.on_primary),
        MethodKind::Neutral => (p.muted, p.card),
    };
    let family = if kind == MethodKind::Neutral { family_for(&label) } else { "Menlo" };
    let (height, pad) = size.dimensions();
    div().h(px(height)).px(px(pad)).flex().flex_none().items_center().justify_center().rounded_full().bg(bg).child(
        div()
            .font_family(family)
            .text_size(px(11.))
            .font_weight(FontWeight::BOLD)
            .line_height(px(14.))
            .text_color(fg)
            .child(label),
    )
}
