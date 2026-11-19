use std::time::Duration;

use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::button::CONTROL_H;
use super::primitives::{icon, mix, transparent};
use super::scope::{ScopeHandle, motion_scope, with_motion_scope};
use super::tween::{RING_MS, count_flips};
use super::typography::text;
use crate::assets::Ic;
use crate::theme::Palette;

pub fn notify_on_focus<V: 'static>(state: &Entity<InputState>, window: &mut Window, cx: &mut Context<V>) {
    let handle = state.read(cx).focus_handle(cx);
    focus_notifier(&handle, window, cx).detach();
}

/// For a handle the view replaces during its life: a detached focus listener stays on the window until the view is
/// released, so the owner of the handle keeps the subscription and drops both together.
pub fn focus_notifier<V: 'static>(handle: &FocusHandle, window: &mut Window, cx: &mut Context<V>) -> Subscription {
    Subscription::join(
        cx.on_focus_in(handle, window, |_, _, cx| cx.notify()),
        cx.on_focus_out(handle, window, |_, _, _, cx| cx.notify()),
    )
}

pub fn new_input<V: 'static>(window: &mut Window, cx: &mut Context<V>, masked: bool) -> Entity<InputState> {
    let state = cx.new(|cx| {
        let input = InputState::new(window, cx);
        if masked { input.masked(true) } else { input }
    });
    notify_on_focus(&state, window, cx);
    state
}

/// Put it as a child of an element that is `.relative().border_1()` with a transparent border (see [`with_ring`]): the
/// outline is an overlay, so nothing inside the element moves when it thickens.
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

/// An invalid field keeps the danger color at rest and in focus, so it reads as invalid whether or not it is being
/// edited.
fn ring_colors(p: &Palette, rest: Hsla, invalid: bool) -> (Hsla, Hsla) {
    if invalid { (p.danger, p.danger) } else { (rest, p.accent) }
}

pub fn field_chrome(id: impl Into<ElementId>, p: &Palette, focused: bool) -> Div {
    field_chrome_with(id, p, focused, false)
}

fn field_chrome_with(id: impl Into<ElementId>, p: &Palette, focused: bool, invalid: bool) -> Div {
    // A 12 px content inset is a third of the 36 px control, the ratio macOS fields use, so the horizontal padding
    // reads as large as the (optically centered) vertical one.
    let base = div().h(px(CONTROL_H)).px(px(12.)).flex().items_center().gap(px(8.)).rounded(px(12.)).bg(p.input_bg);
    let (rest, on) = ring_colors(p, p.border_strong, invalid);
    with_ring(base, id, focused, 12., rest, on)
}

pub(super) fn form_chrome(id: impl Into<ElementId>, p: &Palette, focused: bool, invalid: bool) -> Div {
    let base = div().h(px(40.)).px(px(14.)).flex().items_center().gap(px(8.)).rounded(px(8.)).bg(p.field_bg);
    let (rest, on) = ring_colors(p, p.field_border, invalid);
    with_ring(base, id, focused, 8., rest, on)
}

pub fn text_input(p: &Palette, state: &Entity<InputState>, leading: Option<Ic>) -> TextInput {
    TextInput { p: *p, state: state.clone(), leading, scope: motion_scope(), form: false, invalid: false }
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
    invalid: bool,
}

impl TextInput {
    pub fn form(mut self) -> Self {
        self.form = true;
        self
    }

    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }
}

impl RenderOnce for TextInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self { p, state, leading, scope, form, invalid } = self;
        // Restore the build scope for the deferred render (see the field above).
        with_motion_scope(scope, move || {
            let focused = state.read(cx).focus_handle(cx).contains_focused(window, cx);
            let ring_id = (ElementId::from(("text-input", state.entity_id().as_u64() as usize)), "ring");
            let chrome = if form {
                form_chrome(ring_id, &p, focused, invalid)
            } else {
                field_chrome_with(ring_id, &p, focused, invalid)
            };
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

/// The chrome of [`text_area`]: a fill, an optional edge and the text metrics.
#[derive(Clone, Copy)]
pub struct TextAreaStyle {
    pub bg: Hsla,
    pub edge: Option<Hsla>,
    pub radius: f32,
    pub text_size: f32,
    pub line_height: f32,
    /// Left, top, right and bottom, inside the edge.
    pub pad: [f32; 4],
    /// Fills the parent's size and scrolls its text; `false` grows with the text, within the state's row limits.
    pub fill: bool,
    /// Space kept free at the bottom of a filled area, for a control drawn over it.
    pub reserve_bottom: f32,
}

/// A multi-line editor that keeps focus ownership here, as [`text_input`] does for one line.
pub fn text_area(state: &Entity<TextareaState>, style: TextAreaStyle) -> TextArea {
    TextArea { state: state.clone(), style, focus_edge: None, scope: motion_scope() }
}

#[derive(IntoElement)]
pub struct TextArea {
    state: Entity<TextareaState>,
    style: TextAreaStyle,
    focus_edge: Option<Hsla>,
    scope: ScopeHandle,
}

impl TextArea {
    pub fn focus_ring(mut self, color: Hsla) -> Self {
        self.focus_edge = Some(color);
        self
    }
}

impl RenderOnce for TextArea {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self { state, style, focus_edge, scope } = self;
        with_motion_scope(scope, move || {
            let focus = state.read(cx).focus_handle(cx);
            let focused = focus.contains_focused(window, cx);
            // The editor keeps its own padding (that of the medium input size) around the text, on top of any it is
            // given. A side asking for less is pulled outward with a negative margin, so the text lands exactly at `pad`.
            const EDITOR_PAD: [f32; 4] = [10., 8., 10., 8.];
            let extra: [f32; 4] = std::array::from_fn(|i| style.pad[i] - EDITOR_PAD[i]);
            let [l, t, r, b] = extra.map(|v| px(v.max(0.)));
            let [ml, mt, mr, mb] = extra.map(|v| px(v.min(0.)));
            let editor = Textarea::new(&state)
                .appearance(false)
                .bordered(false)
                .text_size(px(style.text_size))
                .font_weight(FontWeight::NORMAL)
                .line_height(px(style.line_height))
                .pl(l)
                .pr(r)
                .pt(t)
                .pb(b)
                .ml(ml)
                .mr(mr)
                .mt(mt)
                .mb(mb);
            let mut shell = div()
                .id(("text-area", state.entity_id().as_u64() as usize))
                .relative()
                .w_full()
                .rounded(px(style.radius))
                .bg(style.bg)
                .on_mouse_down_out(blur_on_outside(focus.clone()))
                .on_mouse_down(MouseButton::Left, focus_on_click(focus));
            if let Some(on) = focus_edge {
                shell = with_ring(
                    shell,
                    ("text-area-ring", state.entity_id().as_u64() as usize),
                    focused,
                    style.radius,
                    style.edge.unwrap_or(on),
                    on,
                );
            } else if let Some(edge) = style.edge {
                shell = shell.border_1().border_color(edge);
            }
            if style.fill {
                shell.h_full().child(
                    div().absolute().top_0().left_0().right_0().bottom(px(style.reserve_bottom)).child(editor.h_full()),
                )
            } else {
                shell.child(editor)
            }
        })
    }
}

pub fn field(p: &Palette, label: impl Into<SharedString>, control: impl IntoElement) -> Div {
    div().flex().flex_col().gap(px(8.)).child(text(label, 13., FontWeight::MEDIUM, p.text)).child(control)
}

pub fn form_field(p: &Palette, label: impl Into<SharedString>, control: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(text(label, 13., FontWeight::NORMAL, p.muted).line_height(px(20.)))
        .child(control)
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
