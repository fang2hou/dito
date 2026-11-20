//! Shared modal backdrop and focus ownership.

use gpui_kit::prelude::*;
use gpui_kit::*;

pub fn modal_layer(id: impl Into<ElementId>, focus: &FocusHandle) -> Stateful<Div> {
    let focus = focus.clone();
    div()
        .id(id)
        .track_focus(&focus)
        .absolute()
        .inset_0()
        .occlude()
        .bg(hsla(0., 0., 0., 0.3))
        .flex()
        .items_center()
        .justify_center()
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            focus.focus(window, cx);
            cx.stop_propagation();
        })
}
