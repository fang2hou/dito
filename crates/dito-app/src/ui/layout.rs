use gpui_kit::prelude::*;
use gpui_kit::*;

use super::controls::CONTROL_H;
use super::primitives::hex_white;
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
        // Fixed height (tallest control + the former 2×14 padding) so every row in a card
        // lines up regardless of its control; contents are vertically centered.
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
    let mut c = card(p);
    let n = rows.len();
    for (i, r) in rows.into_iter().enumerate() {
        let _ = (i, n);
        c = c.child(r);
    }
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
    let (bg, fg) = if ok { (p.success, hex_white()) } else { (p.danger, hex_white()) };
    solid_tag(label, bg, fg, true)
}

/// Dot + label chip: the colour carries the tone, no background pill. The single status
/// vocabulary on settings cards and permissions (12px label, 7px dot).
pub fn status_chip(label: impl Into<SharedString>, tone: Hsla) -> Div {
    let label = label.into();
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .flex_none()
        .child(div().size(px(7.)).rounded_full().bg(tone))
        .child(text(label, 12., FontWeight::MEDIUM, tone).line_height(relative(1.4)))
}

pub fn page_title(p: &Palette, s: impl Into<SharedString>) -> Div {
    text(s, 28., FontWeight::BOLD, p.text)
}
