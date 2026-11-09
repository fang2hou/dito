use std::time::Duration;

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::button::CONTROL_H;
use super::primitives::{icon, mix, transparent};
use super::scope::{ScopeHandle, motion_scope, with_motion_scope};
use super::tween::{RING_MS, count_flips};
use super::typography::text;
use crate::assets::Ic;
use crate::theme::Palette;

/// Re-render `view` whenever focus enters or leaves an input, so focus rings can animate.
pub fn notify_on_focus<V: 'static>(state: &Entity<InputState>, window: &mut Window, cx: &mut Context<V>) {
    let handle = state.read(cx).focus_handle(cx);
    cx.on_focus_in(&handle, window, |_, _, cx| cx.notify()).detach();
    cx.on_focus_out(&handle, window, |_, _, _, cx| cx.notify()).detach();
}

/// A 1px outline that grows to 2px and shifts to `on_color` while `active`, animated both ways.
/// Put it as a child of an element that is `.relative().border_1()` with a transparent border (see
/// [`with_ring`]): the outline is an overlay, so nothing inside the element moves when it thickens.
pub fn ring_overlay(
    id: impl Into<ElementId>,
    active: bool,
    radius: f32,
    off_color: Hsla,
    on_color: Hsla,
) -> AnyElement {
    let id: ElementId = id.into();
    let flips = count_flips(&id, active, Duration::from_millis(RING_MS));
    let overlay = move |v: f32| {
        div().absolute().inset(px(-1.)).rounded(px(radius)).border(px(1. + v)).border_color(mix(off_color, on_color, v))
    };
    let settled = if active { 1. } else { 0. };
    if flips == 0 {
        return overlay(settled).into_any_element();
    }
    let key = (id, SharedString::from(format!("ring-{flips}")));
    overlay(settled)
        .with_animation(key, Animation::new(Duration::from_millis(RING_MS)), move |_, t| {
            let q = t * t * (3. - 2. * t);
            overlay(if active { q } else { 1. - q })
        })
        .into_any_element()
}

pub fn with_ring<E: Styled + ParentElement>(
    base: E,
    id: impl Into<ElementId>,
    active: bool,
    radius: f32,
    off_color: Hsla,
    on_color: Hsla,
) -> E {
    base.relative().border_1().border_color(transparent()).child(ring_overlay(id, active, radius, off_color, on_color))
}

pub fn field_chrome(id: impl Into<ElementId>, p: &Palette, focused: bool) -> Div {
    // 12 px content inset ≈ a third of the 36 px control — the ratio macOS fields use, so the
    // horizontal padding reads as large as the (optically centered) vertical one.
    let base = div().h(px(CONTROL_H)).px(px(12.)).flex().items_center().gap(px(8.)).rounded(px(12.)).bg(p.input_bg);
    with_ring(base, id, focused, 12., p.border_strong, p.accent)
}

/// The taller connection-form field, on the form field surface.
fn form_chrome(id: impl Into<ElementId>, p: &Palette, focused: bool) -> Div {
    let base = div().h(px(40.)).px(px(14.)).flex().items_center().gap(px(8.)).rounded(px(8.)).bg(p.field_bg);
    with_ring(base, id, focused, 8., p.field_border, p.accent)
}

/// Bordered single-line text field backed by an `InputState`; the border turns accent while it has focus.
pub fn text_input(p: &Palette, state: &Entity<InputState>, leading: Option<Ic>) -> TextInput {
    TextInput { p: *p, state: state.clone(), leading, scope: motion_scope(), form: false }
}

#[derive(IntoElement)]
pub struct TextInput {
    p: Palette,
    state: Entity<InputState>,
    leading: Option<Ic>,
    /// The scope the input was built under: RenderOnce runs after the building page's scope guard
    /// has ended, so the focus ring would otherwise read and write no registry at all.
    scope: ScopeHandle,
    form: bool,
}

impl TextInput {
    /// Use the connection-form look (see [`form_field`]).
    pub fn form(mut self) -> Self {
        self.form = true;
        self
    }
}

impl RenderOnce for TextInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self { p, state, leading, scope, form } = self;
        // Restore the build scope for the deferred render (see the field above).
        with_motion_scope(scope, move || {
            let focused = state.read(cx).focus_handle(cx).contains_focused(window, cx);
            let ring_id = (ElementId::from(("text-input", state.entity_id().as_u64() as usize)), "ring");
            let chrome = if form { form_chrome(ring_id, &p, focused) } else { field_chrome(ring_id, &p, focused) };
            let mut d = chrome.w_full().flex_1().min_w(px(0.));
            if let Some(ic) = leading {
                d = d.child(icon(ic, 14., p.muted));
            }
            d.id(("input-shell", state.entity_id().as_u64() as usize))
                .on_mouse_down_out(blur_on_outside(state.read(cx).focus_handle(cx)))
                .on_mouse_down(MouseButton::Left, focus_on_click(state.read(cx).focus_handle(cx)))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        // Match the select's 13 px label: the component's own sizing (text_sm) runs first,
                        // this refinement wins. Its built-in horizontal padding folds into the chrome's
                        // 12 px inset instead of stacking on top of it.
                        .child(
                            Input::new(&state)
                                .appearance(false)
                                .bordered(false)
                                .focus_bordered(false)
                                .text_size(px(13.))
                                .pl(px(0.))
                                .pr(px(0.)),
                        ),
                )
        })
    }
}

/// Labelled form field: caption above a control.
pub fn field(p: &Palette, label: impl Into<SharedString>, control: impl IntoElement) -> Div {
    div().flex().flex_col().gap(px(8.)).child(text(label, 13., FontWeight::MEDIUM, p.text)).child(control)
}

/// A connection-form field: a quieter caption above a [`TextInput::form`] control.
pub fn form_field(p: &Palette, label: impl Into<SharedString>, control: impl IntoElement) -> Div {
    div().flex().flex_col().gap(px(8.)).child(text(label, 13., FontWeight::NORMAL, p.muted)).child(control)
}

/// Clicking outside a focused field dismisses editing without intercepting the next control's click.
pub fn blur_on_outside(focus: FocusHandle) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| {
        if focus.contains_focused(window, cx) {
            window.blur(cx);
            window.refresh();
        }
    }
}

/// Chrome padding belongs to its field too; inner input handlers retain text selection behavior.
pub fn focus_on_click(focus: FocusHandle) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| {
        focus.focus(window, cx);
        window.refresh();
        cx.stop_propagation();
    }
}
