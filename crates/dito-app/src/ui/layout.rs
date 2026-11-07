use gpui_kit::prelude::*;
use gpui_kit::*;

use super::button::CONTROL_H;
use super::primitives::white;
use super::typography::text;
use crate::theme::{Palette, Tint};

pub fn card(p: &Palette) -> Div {
    div()
        .w_full()
        .relative()
        .flex()
        .flex_col()
        .rounded(px(16.))
        .bg(p.card)
        .border_1()
        .border_color(p.border)
        .child(super::glass::surface())
}

/// One settings row inside a grouped card.
pub fn row(p: &Palette, title: impl Into<SharedString>, control: impl IntoElement, last: bool) -> Div {
    let mut r = div()
        .w_full()
        .px(px(24.))
        // Fixed height so every row in a card lines up regardless of its control; contents are vertically
        // centered.
        .h(px(CONTROL_H + 28.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(24.))
        .child(div().flex_1().min_w(px(0.)).child(text(title, 14., FontWeight::MEDIUM, p.text)))
        .child(div().flex().items_center().gap(px(8.)).flex_none().child(control));
    if !last {
        r = r.border_b_1().border_color(p.border);
    }
    r
}

/// Group heading + card with rows.
pub fn group(p: &Palette, title: impl Into<SharedString>, rows: Vec<Div>) -> Div {
    let c = card(p).children(rows);
    div().w_full().flex().flex_col().gap(px(10.)).child(text(title, 13., FontWeight::BOLD, p.muted)).child(c)
}

pub fn tag(p: &Palette, label: impl Into<SharedString>, tint: Tint) -> Div {
    let (bg, fg) = p.on_tint(tint);
    div()
        .px(px(8.))
        .py(px(2.))
        .rounded(px(6.))
        .bg(bg)
        .flex_none()
        .child(text(label, 12., FontWeight::MEDIUM, fg).line_height(relative(1.4)))
}

/// Solid inverted label, e.g. 使用中 / 稼働中.
pub fn solid_tag(label: impl Into<SharedString>, bg: Hsla, fg: Hsla, dot: bool) -> Div {
    let mut d = div().px(px(10.)).py(px(3.)).rounded_full().bg(bg).flex().items_center().gap(px(6.)).flex_none();
    if dot {
        d = d.child(div().size(px(6.)).rounded_full().bg(fg));
    }
    d.child(text(label, 12., FontWeight::MEDIUM, fg).line_height(relative(1.4)))
}

pub fn status_tag(p: &Palette, ok: bool, label: impl Into<SharedString>) -> Div {
    let (bg, fg) = if ok { (p.success, white()) } else { (p.danger, white()) };
    solid_tag(label, bg, fg, true)
}

/// Semantic colors for the shared inverse labels in 11.03 Controls and 20.08 Status.
#[derive(Clone, Copy)]
pub enum StatusTone {
    Success,
    Warning,
    Danger,
    Neutral,
}

/// A compact status label, with paired palette colors in both appearances.
pub fn status_chip(p: &Palette, label: impl Into<SharedString>, tone: StatusTone) -> Div {
    let (bg, fg) = match tone {
        StatusTone::Success => (p.success, white()),
        StatusTone::Warning => (p.warning_text, p.warning_soft),
        StatusTone::Danger => (p.danger, white()),
        StatusTone::Neutral => (p.muted, p.card),
    };
    div()
        .h(px(24.))
        .px(px(10.))
        .rounded_full()
        .bg(bg)
        .flex()
        .items_center()
        .flex_none()
        .child(text(label, 12., FontWeight::MEDIUM, fg).line_height(px(18.)))
}

pub fn page_title(p: &Palette, s: impl Into<SharedString>) -> Div {
    text(s, 28., FontWeight::BOLD, p.text)
}

/// Empty background that drags the window when pressed: for custom-titlebar zones outside AppKit's
/// native titlebar strip. Only attach to regions that contain no children — a bubble-phase mouse
/// listener on a container would also fire for clicks on everything inside it.
pub fn drag_area(id: impl Into<ElementId>) -> Stateful<Div> {
    div().id(id).on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
}
