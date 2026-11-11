use gpui_kit::base::SelectableText;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::primitives::hex_alpha;
use super::typography::family_for;
use crate::theme::Palette;

/// The window must be rooted in `Root`, which paints the selection and handles the copy.
pub fn selectable_text(
    p: &Palette,
    id: impl Into<ElementId>,
    s: impl Into<SharedString>,
    size: f32,
    weight: FontWeight,
    color: Hsla,
) -> Div {
    let s: SharedString = s.into();
    div()
        .font_family(family_for(&s))
        .text_size(px(size))
        .font_weight(weight)
        .text_color(color)
        .child(SelectableText::new(id, s).selection_color(hex_alpha(p.accent, 0.3)))
}
