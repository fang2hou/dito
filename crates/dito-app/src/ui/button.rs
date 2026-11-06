use gpui_kit::prelude::*;
use gpui_kit::*;

use super::callbacks::{Cb, click_cb};
use super::hover::{hover_fill, hover_tracker};
use super::primitives::icon;
use super::typography::text;
use crate::assets::Ic;
use crate::theme::Palette;

pub const CONTROL_H: f32 = 36.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BtnKind {
    Primary,
    Secondary,
    Danger,
    /// The blue model download button.
    Download,
    /// The model card's 使用する: an outlined pill on the input surface.
    Outline,
    /// A quiet pill on the sub surface, for form actions (編集, 接続テスト).
    Subtle,
}

/// Regular in Settings, Large in onboarding.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BtnSize {
    Regular,
    Large,
}

struct BtnColors {
    bg: Hsla,
    fg: Hsla,
    border: Option<Hsla>,
    tint: Hsla,
}

impl BtnColors {
    fn of(p: &Palette, kind: BtnKind, size: BtnSize) -> Self {
        // A Secondary button sits on the input surface inside cards and on the card surface on a page.
        let secondary_bg = if size == BtnSize::Large { p.card } else { p.input_bg };
        let (bg, fg, border, tint) = match kind {
            BtnKind::Primary => (p.primary, p.on_primary, None, p.button_primary_hover),
            BtnKind::Secondary => (secondary_bg, p.text, Some(p.border_strong), p.seg),
            BtnKind::Danger => {
                (p.button_danger_bg, p.button_danger_text, Some(p.button_danger_border), p.button_danger_hover)
            }
            BtnKind::Download => (p.model_download, p.model_on_download, None, p.button_primary_hover),
            BtnKind::Outline => (p.input_bg, p.text, Some(p.model_use_border), p.nav_on),
            BtnKind::Subtle => (p.sub, p.text, Some(p.field_border), p.seg),
        };
        Self { bg, fg, border, tint }
    }
}

/// Pill button, Regular size.
pub fn button(
    id: impl Into<ElementId>,
    p: &Palette,
    label: &str,
    kind: BtnKind,
    ic: Option<Ic>,
    on_click: Cb,
) -> Stateful<Div> {
    button_with(id, p, label, kind, BtnSize::Regular, ic, None, on_click)
}

/// [`button`] with a size and an optional trailing icon.
#[allow(clippy::too_many_arguments)]
pub fn button_with(
    id: impl Into<ElementId>,
    p: &Palette,
    label: &str,
    kind: BtnKind,
    size: BtnSize,
    lead: Option<Ic>,
    trail: Option<Ic>,
    on_click: Cb,
) -> Stateful<Div> {
    let (h, pad, label_size) = match size {
        BtnSize::Regular => (CONTROL_H, 16., 13.),
        BtnSize::Large => (40., 22., 14.),
    };
    let BtnColors { bg, fg, border, tint } = BtnColors::of(p, kind, size);
    let icon_size = if kind == BtnKind::Download { 16. } else { 14. };
    let id: ElementId = id.into();
    let mut el = div()
        .id(id.clone())
        .h(px(h))
        .px(px(pad))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .relative()
        .rounded_full()
        .bg(bg)
        .cursor_pointer()
        .on_click(click_cb(on_click))
        .on_hover(hover_tracker(id.clone()))
        .child(hover_fill(&id, None, tint));
    if let Some(b) = border {
        el = el.border_1().border_color(b);
    }
    if let Some(ic) = lead {
        el = el.child(icon(ic, icon_size, fg));
    }
    el = el.child(text(label.to_string(), label_size, FontWeight::MEDIUM, fg).line_height(relative(1.)));
    if let Some(ic) = trail {
        el = el.child(icon(ic, icon_size, fg));
    }
    el
}

/// Circular icon button (copy, ⋮ ...).
pub fn icon_button(id: impl Into<ElementId>, p: &Palette, ic: Ic, active: bool, on_click: Cb) -> Stateful<Div> {
    let id: ElementId = id.into();
    div()
        .id(id.clone())
        .size(px(CONTROL_H))
        .flex()
        .items_center()
        .justify_center()
        .relative()
        .rounded_full()
        .border_1()
        .border_color(p.border_strong)
        .when(active, |d| d.bg(p.seg))
        .cursor_pointer()
        .on_click(click_cb(on_click))
        .on_hover(hover_tracker(id.clone()))
        .child(hover_fill(&id, None, p.seg))
        .child(icon(ic, 16., p.text))
}

/// The dialog close button: a cross that gets a surface on hover.
pub fn close_button(id: impl Into<ElementId>, p: &Palette, on_click: Cb) -> Stateful<Div> {
    let id: ElementId = id.into();
    div()
        .id(id.clone())
        .size(px(32.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .relative()
        .rounded_full()
        .cursor_pointer()
        .on_click(click_cb(on_click))
        .on_hover(hover_tracker(id.clone()))
        .child(hover_fill(&id, None, p.sub))
        .child(icon(Ic::X, 24., p.text))
}

/// Plain (borderless) icon button, e.g. the × beside a sample row.
pub fn bare_icon_button(id: impl Into<ElementId>, p: &Palette, ic: Ic, on_click: Cb) -> Stateful<Div> {
    let id: ElementId = id.into();
    div()
        .id(id.clone())
        .size(px(CONTROL_H))
        .flex()
        .items_center()
        .justify_center()
        .relative()
        .rounded_full()
        .cursor_pointer()
        .on_click(click_cb(on_click))
        .on_hover(hover_tracker(id.clone()))
        .child(hover_fill(&id, None, p.seg))
        .child(icon(ic, 16., p.muted))
}
