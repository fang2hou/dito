use gpui_kit::prelude::*;
use gpui_kit::*;

use super::callbacks::{Cb, click_cb};
use super::hover::{hover_fill, hover_tracker};
use super::primitives::icon;
use super::typography::text_tracked;
use crate::assets::Ic;
use crate::theme::Palette;

pub const CONTROL_H: f32 = 36.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BtnKind {
    Primary,
    Secondary,
    Danger,
    Destructive,
    Subtle,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BtnSize {
    Regular,
    Large,
    Hero,
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
            BtnKind::Destructive => (p.danger, super::primitives::white(), None, p.button_primary_hover),
            BtnKind::Subtle => (p.sub, p.text, Some(p.field_border), p.seg),
        };
        Self { bg, fg, border, tint }
    }
}

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
    button_impl(id, p, label, kind, size, lead, trail, Some(on_click))
}

/// Shares label, icon and width calculation with the active button.
#[allow(clippy::too_many_arguments)]
pub fn disabled_button(
    id: impl Into<ElementId>,
    p: &Palette,
    label: &str,
    kind: BtnKind,
    size: BtnSize,
    lead: Option<Ic>,
    trail: Option<Ic>,
) -> Stateful<Div> {
    button_impl(id, p, label, kind, size, lead, trail, None)
}

#[allow(clippy::too_many_arguments)]
fn button_impl(
    id: impl Into<ElementId>,
    p: &Palette,
    label: &str,
    kind: BtnKind,
    size: BtnSize,
    lead: Option<Ic>,
    trail: Option<Ic>,
    on_click: Option<Cb>,
) -> Stateful<Div> {
    let (h, pad, label_size, lead_size, trail_size) = match size {
        BtnSize::Regular => (CONTROL_H, 16., 13., 16., 14.),
        BtnSize::Large => (40., 22., 14., 18., 16.),
        BtnSize::Hero => (50., 40., 15., 20., 18.),
    };
    let BtnColors { bg, fg, border, tint } = BtnColors::of(p, kind, size);
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
        .bg(bg);
    if let Some(on_click) = on_click {
        el = el
            .cursor_pointer()
            .on_click(click_cb(on_click))
            .on_hover(hover_tracker(id.clone()))
            .child(hover_fill(&id, None, tint));
    }
    if let Some(b) = border {
        el = el.border_1().border_color(b);
    }
    if let Some(ic) = lead {
        el = el.child(icon(ic, lead_size, fg));
    }
    let tracking = if size == BtnSize::Regular { 0.05 } else { 0.04 };
    el = el.child(text_tracked(label, label_size, FontWeight::MEDIUM, fg, tracking));
    if let Some(ic) = trail {
        el = el.child(icon(ic, trail_size, fg));
    }
    el
}

pub fn icon_button(id: impl Into<ElementId>, p: &Palette, ic: Ic, active: bool, on_click: Cb) -> Stateful<Div> {
    icon_button_with(id, p, ic, active, CircleSize::Regular, on_click)
}

/// The design's disabled state: the same instance at 45% opacity.
pub fn disabled_icon_button(id: impl Into<ElementId>, p: &Palette, ic: Ic) -> Stateful<Div> {
    circle_control(
        id,
        ic,
        CircleSize::Regular,
        CircleAppearance { background: None, foreground: p.text, border: Some(p.border_strong), hover: None },
    )
    .opacity(0.45)
}

/// Circle and glyph pairs from `design:components/control-icon-contracts`.
#[derive(Clone, Copy)]
pub enum CircleSize {
    Compact,
    Small,
    Regular,
    #[expect(dead_code, reason = "Approved 40/22 template, reserved for large circular actions")]
    Large,
}

impl CircleSize {
    pub const fn dimensions(self) -> (f32, f32) {
        match self {
            Self::Compact => (24., 14.),
            Self::Small => (32., 18.),
            Self::Regular => (36., 20.),
            Self::Large => (40., 22.),
        }
    }
}

pub fn icon_button_with(
    id: impl Into<ElementId>,
    p: &Palette,
    ic: Ic,
    active: bool,
    size: CircleSize,
    on_click: Cb,
) -> Stateful<Div> {
    circle_control(
        id,
        ic,
        size,
        CircleAppearance {
            background: active.then_some(p.seg),
            foreground: p.text,
            border: Some(p.border_strong),
            hover: Some(p.seg),
        },
    )
    .cursor_pointer()
    .on_click(click_cb(on_click))
}

pub struct CircleAppearance {
    pub background: Option<Hsla>,
    pub foreground: Hsla,
    pub border: Option<Hsla>,
    pub hover: Option<Hsla>,
}

pub fn circle_control(
    id: impl Into<ElementId>,
    ic: Ic,
    size: CircleSize,
    appearance: CircleAppearance,
) -> Stateful<Div> {
    let id = id.into();
    let (diameter, glyph) = size.dimensions();
    let mut control = div()
        .id(id.clone())
        .size(px(diameter))
        .flex_none()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .on_hover(hover_tracker(id.clone()));
    if let Some(background) = appearance.background {
        control = control.bg(background);
    }
    if let Some(border) = appearance.border {
        control = control.border_1().border_color(border);
    }
    if let Some(hover) = appearance.hover {
        control = control.child(hover_fill(&id, None, hover));
    }
    control.child(icon(ic, glyph, appearance.foreground))
}

pub fn close_button(id: impl Into<ElementId>, p: &Palette, on_click: Cb) -> Stateful<Div> {
    circle_control(
        id,
        Ic::X,
        CircleSize::Small,
        CircleAppearance { background: None, foreground: p.text, border: None, hover: Some(p.sub) },
    )
    .cursor_pointer()
    .on_click(click_cb(on_click))
}

pub fn bare_icon_button(id: impl Into<ElementId>, p: &Palette, ic: Ic, on_click: Cb) -> Stateful<Div> {
    circle_control(
        id,
        ic,
        CircleSize::Regular,
        CircleAppearance { background: None, foreground: p.muted, border: None, hover: Some(p.seg) },
    )
    .cursor_pointer()
    .on_click(click_cb(on_click))
}
