use gpui_kit::base::SelectableText;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::primitives::hex_alpha;
use super::typography::family_for;
use crate::theme::Palette;

/// [`text`](super::text) the person can select with the pointer and copy with Cmd-C: for values worth pasting
/// elsewhere, such as a file path or an error message. Set `font_family` or `line_height` on the result as for `text`.
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
