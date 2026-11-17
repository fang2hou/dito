//! The confirmation dialog (`design:settings/prompts`, `design:settings/cloud-models/delete-configuration`).

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::button::{BtnKind, BtnSize, button, disabled_button};
use super::callbacks::Cb;
use super::hover::reset_hover_in;
use super::modal::modal_layer;
use super::scope::motion_scope;
use super::typography::text;
use crate::assets::Ic;
use crate::theme::Palette;

pub struct DialogAnswer {
    pub label: SharedString,
    /// `None` shows the answer as unavailable.
    pub on_click: Option<Cb>,
}

#[derive(Clone, Copy)]
pub enum ConfirmStyle {
    Standard,
    CompactDestructive,
}

/// Cancel is also what Escape does. Without `confirm` the dialog has that one answer. `focus` owns the keyboard while the dialog is open; the caller focuses it when it
/// opens the dialog.
#[allow(clippy::too_many_arguments)]
pub fn confirm_dialog(
    window: &Window,
    p: &Palette,
    id: &'static str,
    focus: &FocusHandle,
    title: impl Into<SharedString>,
    content: Vec<AnyElement>,
    cancel: DialogAnswer,
    confirm: Option<DialogAnswer>,
    style: ConfirmStyle,
) -> AnyElement {
    let header = text(title, 20., FontWeight::BOLD, p.text).into_any_element();
    confirm_dialog_with_header(window, p, id, focus, header, content, cancel, confirm, style)
}

/// [`confirm_dialog`] with the caller's first row in place of a plain title, for a title that shares its row with a
/// control.
#[allow(clippy::too_many_arguments)]
pub fn confirm_dialog_with_header(
    window: &Window,
    p: &Palette,
    id: &'static str,
    focus: &FocusHandle,
    header: AnyElement,
    content: Vec<AnyElement>,
    cancel: DialogAnswer,
    confirm: Option<DialogAnswer>,
    style: ConfirmStyle,
) -> AnyElement {
    let (width, padding, action_gap, confirm_kind) = match style {
        ConfirmStyle::Standard => (620., 28., 10., BtnKind::Primary),
        ConfirmStyle::CompactDestructive => (524., 24., 12., BtnKind::Destructive),
    };
    let escape = cancel.on_click.clone();
    let actions = std::iter::once(dialog_button(
        p,
        ElementId::from((ElementId::from(id), "cancel")),
        BtnKind::Secondary,
        None,
        cancel,
    ))
    .chain(confirm.map(|confirm| {
        dialog_button(p, ElementId::from((ElementId::from(id), "confirm")), confirm_kind, None, confirm)
    }))
    .map(IntoElement::into_any_element)
    .collect();
    shell(window, p, id, focus, Frame { width, padding, action_gap }, header, content, actions, escape)
}

/// A Standard dialog whose answers the caller builds, for answers a [`DialogAnswer`] cannot express (an icon, more
/// than two). `escape` is what Escape does; make each answer with [`dialog_button`].
#[allow(clippy::too_many_arguments)]
pub fn dialog_with_actions(
    window: &Window,
    p: &Palette,
    id: &'static str,
    focus: &FocusHandle,
    header: AnyElement,
    content: Vec<AnyElement>,
    actions: Vec<AnyElement>,
    escape: Option<Cb>,
) -> AnyElement {
    shell(window, p, id, focus, Frame { width: 620., padding: 28., action_gap: 10. }, header, content, actions, escape)
}

struct Frame {
    width: f32,
    padding: f32,
    action_gap: f32,
}

#[allow(clippy::too_many_arguments)]
fn shell(
    window: &Window,
    p: &Palette,
    id: &'static str,
    focus: &FocusHandle,
    frame: Frame,
    header: AnyElement,
    content: Vec<AnyElement>,
    actions: Vec<AnyElement>,
    escape: Option<Cb>,
) -> AnyElement {
    let body = div()
        .w(px(frame.width))
        .max_w(window.viewport_size().width - px(48.))
        .p(px(frame.padding))
        .rounded(px(20.))
        .border_1()
        .border_color(p.border_strong)
        .bg(p.card)
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(header)
        .children(content)
        .child(div().w_full().flex().justify_end().gap(px(frame.action_gap)).children(actions));
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

/// One answer of a dialog; `None` in `answer.on_click` shows it as unavailable.
pub fn dialog_button(
    p: &Palette,
    id: ElementId,
    kind: BtnKind,
    lead: Option<Ic>,
    answer: DialogAnswer,
) -> Stateful<Div> {
    let DialogAnswer { label, on_click } = answer;
    let Some(on_click) = on_click else {
        return disabled_button(id, p, &label, kind, BtnSize::Regular, lead, None);
    };
    // The dialog goes away under the pointer, which never reports the end of the hover. Forget it here so the next
    // dialog does not open with this button lit.
    let (scope, hovered) = (motion_scope(), id.clone());
    button(
        id,
        p,
        &label,
        kind,
        lead,
        std::rc::Rc::new(move |window, cx| {
            reset_hover_in(scope.clone(), &hovered);
            on_click(window, cx);
        }),
    )
}

/// The tool settings dialog of `design:components` (11.03.14): a centered 480 pt dialog with the tool's title, a ghost
/// close button and the tool's own settings. Every change saves as it is made, so closing is the only answer; the
/// close button and Escape both call `on_close`.
pub fn tool_settings_dialog(
    window: &Window,
    p: &Palette,
    id: &'static str,
    focus: &FocusHandle,
    title: impl Into<SharedString>,
    content: AnyElement,
    on_close: Cb,
) -> AnyElement {
    tool_settings_dialog_with(window, p, id, focus, ToolDialogHeader::titled(480., title), content, on_close)
}

/// The header of a tool settings dialog: its width and the title with the two optional slots of the design, one before
/// the title (a Method Label) and the actions beside the close button.
pub struct ToolDialogHeader {
    pub width: f32,
    pub title: SharedString,
    pub leading: Option<AnyElement>,
    pub actions: Option<AnyElement>,
}

impl ToolDialogHeader {
    pub fn titled(width: f32, title: impl Into<SharedString>) -> Self {
        Self { width, title: title.into(), leading: None, actions: None }
    }
}

pub fn tool_settings_dialog_with(
    window: &Window,
    p: &Palette,
    id: &'static str,
    focus: &FocusHandle,
    header: ToolDialogHeader,
    content: AnyElement,
    on_close: Cb,
) -> AnyElement {
    let escape = on_close.clone();
    let ToolDialogHeader { width, title, leading, actions } = header;
    let header = div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(12.))
        .children(leading)
        .child(div().flex_1().min_w(px(0.)).child(text(title, 20., FontWeight::BOLD, p.text)))
        .children(actions)
        .child(super::button::close_button(ElementId::from((ElementId::from(id), "close")), p, on_close));
    let body = div()
        .w(px(width))
        .max_w(window.viewport_size().width - px(48.))
        .p(px(28.))
        .rounded(px(20.))
        .border_1()
        .border_color(p.border_strong)
        .bg(p.card)
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(header)
        .child(content);
    modal_layer(id, focus)
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" {
                escape(window, cx);
                cx.stop_propagation();
            }
        })
        .child(body)
        .into_any_element()
}
