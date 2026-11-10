//! The parameter block of a model (20.05.7 cloud, 20.04.2 local): a head line with a title and a hint, one row of equal
//! form fields and, below it, fields of the full width. The caller supplies the labels, the inputs (with their
//! placeholders) and which of them hold text that does not parse.

use gpui_kit::component::input::InputState;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::inputs::{form_field, text_input};
use super::typography::text;
use crate::theme::Palette;

/// One field of a [`parameter_block`].
pub struct ParamField {
    label: SharedString,
    input: Entity<InputState>,
    invalid: bool,
}

impl ParamField {
    pub fn new(label: impl Into<SharedString>, input: &Entity<InputState>) -> Self {
        Self { label: label.into(), input: input.clone(), invalid: false }
    }

    /// The text does not parse: the field takes the danger border.
    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    fn view(self, p: &Palette) -> Div {
        form_field(p, self.label, text_input(p, &self.input, None).form().invalid(self.invalid))
    }
}

/// The title (13 Medium) and the hint (13 Regular, muted) 12 apart, then `row` as equal fields 16 apart, then each
/// of `wide` across the full width, 12 below the one before.
pub fn parameter_block(
    p: &Palette,
    title: impl Into<SharedString>,
    hint: impl Into<SharedString>,
    row: Vec<ParamField>,
    wide: Vec<ParamField>,
) -> Div {
    let head = div()
        .w_full()
        .flex()
        .items_start()
        .gap(px(12.))
        .child(text(title, 13., FontWeight::MEDIUM, p.text).line_height(px(20.)).flex_none())
        .child(text(hint, 13., FontWeight::NORMAL, p.muted).line_height(px(20.)).flex_1().min_w(px(0.)));
    let fields = row
        .into_iter()
        .fold(div().w_full().flex().gap(px(16.)), |fields, field| fields.child(field.view(p).flex_1().min_w(px(0.))));
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(head)
        .child(fields)
        .children(wide.into_iter().map(|field| field.view(p).w_full()))
}
