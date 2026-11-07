//! Circular model controls and status markers from 20.04 Local Models.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::Ic;
use crate::theme::Palette;
use crate::ui::{self, BtnKind, Cb};

pub(super) fn model_action(
    id: impl Into<ElementId>,
    p: &Palette,
    label: &str,
    kind: BtnKind,
    ic: Ic,
    on_click: Cb,
) -> Stateful<Div> {
    let id = id.into();
    ui::tip_label(id.clone(), label.to_owned().into());
    let (bg, fg, border, hover) = match kind {
        BtnKind::Danger => (p.button_danger_bg, p.button_danger_text, p.button_danger_border, p.button_danger_hover),
        _ => (p.card, p.text, p.border_strong, p.seg),
    };
    let click_id = id.clone();
    let scope = ui::motion_scope();
    control(id.clone(), bg, border)
        .cursor_pointer()
        .child(ui::hover_fill(&id, None, hover))
        .child(action_icon(ic, fg))
        .child(ui::tip_above(p, &id))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, w, cx| {
            cx.stop_propagation();
            ui::reset_hover_in(scope.clone(), &click_id);
            on_click(w, cx);
        })
}

pub(super) fn in_use_status(id: impl Into<ElementId>, p: &Palette, label: SharedString) -> Stateful<Div> {
    let id = id.into();
    ui::tip_label(id.clone(), label);
    control(id.clone(), p.model_in_use_pill_bg, p.model_in_use_pill_bg)
        .child(action_icon(Ic::Check, p.model_in_use_pill_text))
        .child(ui::tip_above(p, &id))
}

fn control(id: ElementId, bg: Hsla, border: Hsla) -> Stateful<Div> {
    div()
        .id(id.clone())
        .size(px(36.))
        .flex_none()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(bg)
        .border_1()
        .border_color(border)
        .on_hover(ui::hover_tracker(id.clone()))
}

fn action_icon(ic: Ic, fg: Hsla) -> Div {
    let glyph = match ic {
        Ic::X => svg().path("icons/model-cancel.svg").size(px(24.)).flex_none().text_color(fg),
        Ic::Download => svg().path("icons/model-download.svg").size(px(17.5)).flex_none().text_color(fg),
        _ => ui::icon(ic, 20., fg),
    };
    div().flex().items_center().justify_center().relative().when(ic == Ic::Check, |d| d.top(px(0.5))).child(glyph)
}
