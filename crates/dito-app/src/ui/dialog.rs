//! The confirmation dialog (`design:settings/prompts`, `design:models/cloud/delete-configuration`): a card over the dimmed window with a title,
//! some content and, right-aligned, the answer that backs out and the one that goes ahead.

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::button::{BtnKind, BtnSize, button, disabled_button};
use super::callbacks::Cb;
use super::hover::reset_hover_in;
use super::modal::modal_layer;
use super::scope::motion_scope;
use super::typography::text;
use crate::theme::Palette;

/// One of the two answers of a dialog.
pub struct DialogAnswer {
    pub label: SharedString,
    /// What it does. `None` shows the answer as unavailable.
    pub on_click: Option<Cb>,
}

#[derive(Clone, Copy)]
pub enum ConfirmStyle {
    Standard,
    CompactDestructive,
}

/// The dialog `id` with its `content` between the title and the answers. Cancel is also what Escape does. `focus` is
/// the handle that owns the keyboard while the dialog is open; the caller focuses it when it opens the dialog.
#[allow(clippy::too_many_arguments)]
pub fn confirm_dialog(
    window: &Window,
    p: &Palette,
    id: &'static str,
    focus: &FocusHandle,
    title: impl Into<SharedString>,
    content: Vec<AnyElement>,
    cancel: DialogAnswer,
    confirm: DialogAnswer,
    style: ConfirmStyle,
) -> AnyElement {
    let (width, padding, action_gap, confirm_kind) = match style {
        ConfirmStyle::Standard => (620., 28., 10., BtnKind::Primary),
        ConfirmStyle::CompactDestructive => (524., 24., 12., BtnKind::Destructive),
    };
    let escape = cancel.on_click.clone();
    let body = div()
        .w(px(width))
        .max_w(window.viewport_size().width - px(48.))
        .p(px(padding))
        .rounded(px(20.))
        .border_1()
        .border_color(p.border_strong)
        .bg(p.card)
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(text(title, 20., FontWeight::BOLD, p.text))
        .children(content)
        .child(
            div()
                .w_full()
                .flex()
                .justify_end()
                .gap(px(action_gap))
                .child(answer(p, ElementId::from((ElementId::from(id), "cancel")), BtnKind::Secondary, cancel))
                .child(answer(p, ElementId::from((ElementId::from(id), "confirm")), confirm_kind, confirm)),
        );
    modal_layer(id, focus)
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape"
                && let Some(cancel) = &escape
            {
                cancel(window, cx);
                cx.stop_propagation();
            }
        })
        .child(body)
        .into_any_element()
}

fn answer(p: &Palette, id: ElementId, kind: BtnKind, answer: DialogAnswer) -> Stateful<Div> {
    let DialogAnswer { label, on_click } = answer;
    let Some(on_click) = on_click else {
        return disabled_button(id, p, &label, kind, BtnSize::Regular, None, None).opacity(0.5);
    };
    // The dialog goes away under the pointer, which never reports the end of the hover: forget it here, so the next
    // dialog does not open with this button lit.
    let (scope, hovered) = (motion_scope(), id.clone());
    button(
        id,
        p,
        &label,
        kind,
        None,
        std::rc::Rc::new(move |window, cx| {
            reset_hover_in(scope.clone(), &hovered);
            on_click(window, cx);
        }),
    )
}
