//! The footer of a cloud configuration form (`design:models/cloud/speech-form`, `design:models/cloud/failed-test`): under a rule, the state of the connection test on the
//! left and, on the right, the trash that deletes the configuration, the test button and whatever else the form puts
//! there. A trash with nothing to delete and a test that cannot start are the shared disabled controls: they look the
//! part and do nothing when pressed.

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::button::{BtnKind, BtnSize, CircleSize, button, disabled_button, disabled_icon_button, icon_button_with};
use super::callbacks::Cb;
use super::tooltip::{tip_above, tip_label};
use crate::assets::Ic;
use crate::theme::Palette;

/// The design's `opacity/disabled`: a disabled control is the same instance at 45 percent.
const DISABLED: f32 = 0.45;

/// The trash button of a footer.
pub struct FooterTrash {
    pub id: &'static str,
    /// The label that appears over it while the pointer rests on it. `None` for no tooltip.
    pub tip: Option<SharedString>,
    /// What it does. `None` when there is nothing to delete: the button is shown disabled.
    pub on_click: Option<Cb>,
}

/// The connection test button of a footer.
pub struct TestButton {
    pub id: &'static str,
    pub label: SharedString,
    /// A fixed width; the width of the label when `None`.
    pub width: Option<f32>,
    /// What it does. `None` when a test cannot start now: the button is shown disabled.
    pub on_click: Option<Cb>,
}

/// The footer of a form: `status` on the left, taking what the buttons leave, then the trash, the test button and the
/// `trailing` controls, 10 apart.
pub fn form_footer(p: &Palette, status: Div, trash: FooterTrash, test: TestButton, trailing: Vec<AnyElement>) -> Div {
    div()
        .w_full()
        .pt(px(16.))
        .border_t_1()
        .border_color(p.border)
        .flex()
        .items_center()
        .gap(px(16.))
        .child(status.flex_1().min_w(px(0.)))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(trash_button(p, trash))
                .child(test_button(p, test))
                .children(trailing),
        )
}

fn trash_button(p: &Palette, trash: FooterTrash) -> Stateful<Div> {
    let FooterTrash { id, tip, on_click } = trash;
    let Some(on_click) = on_click else { return disabled_icon_button(id, p, Ic::Trash) };
    let button = icon_button_with(id, p, Ic::Trash, false, CircleSize::Regular, on_click);
    let Some(label) = tip else { return button };
    let key = ElementId::from(id);
    tip_label(key.clone(), label);
    button.child(tip_above(p, &key))
}

fn test_button(p: &Palette, test: TestButton) -> Stateful<Div> {
    let TestButton { id, label, width, on_click } = test;
    let control = match on_click {
        Some(on_click) => button(id, p, &label, BtnKind::Subtle, None, on_click),
        None => disabled_button(id, p, &label, BtnKind::Subtle, BtnSize::Regular, None, None).opacity(DISABLED),
    };
    control.when_some(width, |control, width| control.w(px(width)))
}
