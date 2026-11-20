//! The parameter block of a model (`design:settings/cloud-models/configurations`,
//! `design:settings/local-models/parameters`).

use gpui_kit::component::input::InputState;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::inputs::{form_field, text_input};
use super::typography::text;
use crate::Palette;

pub struct ParamField {
    label: SharedString,
    input: Entity<InputState>,
    invalid: bool,
}

impl ParamField {
    pub fn new(label: impl Into<SharedString>, input: &Entity<InputState>) -> Self {
        Self { label: label.into(), input: input.clone(), invalid: false }
    }

    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    fn view(self, p: &Palette) -> Div {
        form_field(p, self.label, text_input(p, &self.input, None).form().invalid(self.invalid))
    }
}

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
