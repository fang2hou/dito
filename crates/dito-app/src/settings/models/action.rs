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
    ui::circle_control(
        id.clone(),
        ic,
        ui::CircleSize::Regular,
        ui::CircleAppearance { background: Some(bg), foreground: fg, border: Some(border), hover: Some(hover) },
    )
    .cursor_pointer()
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
    ui::circle_control(
        id.clone(),
        Ic::Check,
        ui::CircleSize::Regular,
        ui::CircleAppearance {
            background: Some(p.model_in_use_pill_bg),
            foreground: p.model_in_use_pill_text,
            border: Some(p.model_in_use_pill_bg),
            hover: None,
        },
    )
    .child(ui::tip_above(p, &id))
}
