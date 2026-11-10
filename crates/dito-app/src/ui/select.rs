use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::button::CONTROL_H;
use super::callbacks::{Cb, IndexCb};
use super::hover::{HOVER_MS, hover_fill, hover_tracker, is_hovered};
use super::inputs::{field_chrome, form_chrome};
use super::menu_button::{press_mark, press_toggle};
use super::menu_panel::{float, menu_panel, menu_row, menu_row_staying};
use super::primitives::icon;
use super::scope::{ScopeHandle, motion_scope};
use super::select_badge::{SelectBadge, select_badge};
use super::tween::{RING_MS, count_flips, tweened};
use super::typography::text;
use crate::assets::Ic;
use crate::theme::Palette;

pub struct SelectItem {
    pub label: SharedString,
    pub badge: Option<SelectBadge>,
}

/// The select's arrow: points down when closed and turns to point up while the menu is open, on the same
/// clock as the menu's appearance.
fn chevron(id: ElementId, color: Hsla, open: bool) -> AnyElement {
    let flips = count_flips(&id, open, Duration::from_millis(RING_MS));
    let at = move |f: f32| {
        icon(Ic::ChevronDown, 14., color).with_transformation(Transformation::rotate(radians(std::f32::consts::PI * f)))
    };
    let settled = if open { 1. } else { 0. };
    if flips == 0 {
        return at(settled).into_any_element();
    }
    at(settled)
        .with_animation(
            (id, SharedString::from(format!("turn-{flips}"))),
            Animation::new(Duration::from_millis(RING_MS)),
            move |_, t| {
                let q = t * t * (3. - 2. * t);
                at(if open { q } else { 1. - q })
            },
        )
        .into_any_element()
}

/// How a select is drawn: the standard 36 pt control, or the 40 pt control of connection forms that sits beside the
/// form fields (see `form_field`). Only the chrome and its text size differ; the menu and the motion are the same.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    Standard,
    Form,
}

impl Look {
    fn radius(self) -> f32 {
        match self {
            Self::Standard => 12.,
            Self::Form => 8.,
        }
    }

    fn height(self) -> f32 {
        match self {
            Self::Standard => CONTROL_H,
            Self::Form => 40.,
        }
    }

    fn label_size(self) -> f32 {
        match self {
            Self::Standard => 14.,
            Self::Form => 13.,
        }
    }

    fn chrome(self, ring: (ElementId, &'static str), p: &Palette, focused: bool) -> Div {
        match self {
            Self::Standard => field_chrome(ring, p, focused),
            Self::Form => form_chrome(ring, p, focused, false),
        }
    }
}

/// The select's hover outline: a hairline in `color` that fades in over the resting
/// border while hovered, and stays out of the way while the menu is open (the accent ring takes over).
fn hover_border(id: &ElementId, radius: f32, color: Hsla, open: bool) -> AnyElement {
    let target = if !open && is_hovered(id) { 1. } else { 0. };
    tweened(ElementId::from((id.clone(), "hover-border")), target, HOVER_MS, move |v| {
        div().absolute().inset(px(-1.)).rounded(px(radius)).border_1().border_color(Hsla { a: color.a * v, ..color })
    })
}

/// An inert select in the connection-form look: same chrome, muted content, no menu. For the "nothing to pick" case
/// (e.g. no AWS profiles configured) — the control must not look operable.
pub fn select_form_disabled(p: &Palette, label: SharedString) -> Div {
    let look = Look::Form;
    look.chrome((ElementId::from("select-disabled"), "ring"), p, false)
        .w_full()
        .px(px(14.))
        .gap(px(10.))
        .justify_between()
        .opacity(0.6)
        .child(text(label, look.label_size(), FontWeight::NORMAL, p.muted).line_height(px(20.)))
        .child(icon(Ic::ChevronDown, 14., p.muted))
}

/// Drop-down: a bordered button that opens a list below it. The menu is never narrower than the
/// button; longer options (device names) may widen it beyond.
#[allow(clippy::too_many_arguments)]
pub fn select(
    id: &'static str,
    p: &Palette,
    options: &[SharedString],
    selected: usize,
    open: bool,
    width: Option<f32>,
    on_toggle: Cb,
    on_pick: IndexCb,
) -> Div {
    let items: Vec<SelectItem> = options.iter().map(|o| SelectItem { label: o.clone(), badge: None }).collect();
    select_items(id, p, &items, selected, open, width, on_toggle, on_pick)
}

/// Shared trigger for plain and application selects. Content varies; chrome, hover, arrow and toggling do not.
pub fn select_trigger(
    id: &'static str,
    p: &Palette,
    open: bool,
    content: impl IntoElement,
    on_toggle: Cb,
) -> Stateful<Div> {
    trigger(Look::Standard, id, p, open, content, on_toggle)
}

fn trigger(
    look: Look,
    id: &'static str,
    p: &Palette,
    open: bool,
    content: impl IntoElement,
    on_toggle: Cb,
) -> Stateful<Div> {
    let scope = motion_scope();
    look.chrome((ElementId::from(id), "ring"), p, open)
        .id(id)
        .px(px(14.))
        .cursor_pointer()
        .justify_between()
        .gap(px(10.))
        .on_hover(hover_tracker(ElementId::from(id)))
        .child(hover_fill(&ElementId::from(id), Some(look.radius()), p.select_hover))
        .child(hover_border(&ElementId::from(id), look.radius(), p.select_hover_border, open))
        .on_mouse_down(MouseButton::Left, press_mark(scope.clone(), ElementId::from(id), open))
        .on_click(press_toggle(scope, ElementId::from(id), on_toggle))
        .child(content)
        .child(chevron(ElementId::from((ElementId::from(id), "chevron")), p.muted, open))
}

/// Fill-width buttons resolve their width at layout time; record it every paint so the menu
/// panel (built one frame ahead) can match it. The paint closure captures the build scope's
/// handle: the ambient scope no longer applies while painting.
fn width_probe(id: &'static str, scope: ScopeHandle) -> impl IntoElement {
    canvas(
        move |bounds, _, _| {
            if let Some(reg) = scope.registry() {
                reg.borrow_mut().select_w.insert(id, f32::from(bounds.size.width));
            }
        },
        |_, _, _, _| (),
    )
    .absolute()
    .size_full()
}

#[allow(clippy::too_many_arguments)]
fn select_row(
    id: &'static str,
    p: &Palette,
    i: usize,
    item: &SelectItem,
    on: bool,
    close_on_pick: bool,
    on_pick: &IndexCb,
    on_toggle: Option<&Cb>,
) -> Stateful<Div> {
    let (pick, toggle) = (on_pick.clone(), on_toggle.cloned());
    let row = if close_on_pick {
        menu_row(ElementId::from((id, i)), p)
    } else {
        menu_row_staying(ElementId::from((id, i)), p)
    };
    row.justify_between()
        .on_click(move |_, w, cx| {
            pick(i, w, cx);
            if let Some(toggle) = &toggle {
                toggle(w, cx);
            }
        })
        // The label takes the leftover width and truncates; badge and check keep their own
        // fixed slots, so a long name can never run under them.
        .child(
            text(item.label.clone(), 13., FontWeight::NORMAL, p.text)
                .line_height(px(20.))
                .flex_1()
                .min_w(px(0.))
                .truncate(),
        )
        .children(item.badge.as_ref().map(|b| select_badge((id, i), p, b)))
        .child(div().size(px(14.)).flex_none().when(on, |d| d.child(icon(Ic::Check, 14., p.accent_text))))
}

/// [`select`] with per-row badges.
#[allow(clippy::too_many_arguments)]
pub fn select_items(
    id: &'static str,
    p: &Palette,
    options: &[SelectItem],
    selected: usize,
    open: bool,
    width: Option<f32>,
    on_toggle: Cb,
    on_pick: IndexCb,
) -> Div {
    let label = options.get(selected).map(|o| o.label.clone()).unwrap_or_default();
    select_control(Look::Standard, id, p, options, |i| i == selected, label, open, width, on_toggle, on_pick, true)
}

/// [`select`] in the connection-form look: as wide as its parent and 40 pt high, beside the form fields.
#[allow(clippy::too_many_arguments)]
pub fn select_form(
    id: &'static str,
    p: &Palette,
    options: &[SharedString],
    selected: usize,
    open: bool,
    on_toggle: Cb,
    on_pick: IndexCb,
) -> Div {
    let items: Vec<SelectItem> = options.iter().map(|o| SelectItem { label: o.clone(), badge: None }).collect();
    let label = options.get(selected).cloned().unwrap_or_default();
    select_control(Look::Form, id, p, &items, |i| i == selected, label, open, None, on_toggle, on_pick, true)
}

/// Multi-select with the same menu and checkmarks. Picking an option keeps the menu open.
#[allow(clippy::too_many_arguments)]
pub fn select_multi(
    id: &'static str,
    p: &Palette,
    options: &[SharedString],
    selected: &[bool],
    label: SharedString,
    open: bool,
    width: f32,
    on_toggle: Cb,
    on_pick: IndexCb,
) -> Div {
    let items: Vec<_> = options.iter().map(|label| SelectItem { label: label.clone(), badge: None }).collect();
    select_control(Look::Standard, id, p, &items, |i| selected[i], label, open, Some(width), on_toggle, on_pick, false)
}

#[allow(clippy::too_many_arguments)]
fn select_control(
    look: Look,
    id: &'static str,
    p: &Palette,
    options: &[SelectItem],
    selected: impl Fn(usize) -> bool,
    label: SharedString,
    open: bool,
    width: Option<f32>,
    on_toggle: Cb,
    on_pick: IndexCb,
    close_on_pick: bool,
) -> Div {
    let scope = motion_scope();
    let button = trigger(
        look,
        id,
        p,
        open,
        text(label, look.label_size(), FontWeight::NORMAL, p.text)
            .line_height(px(20.))
            .flex_1()
            .min_w(px(0.))
            .truncate(),
        on_toggle.clone(),
    );
    let button = match width {
        Some(width) => button.w(px(width)),
        None => button.w_full(),
    };
    let mut root = div().relative().when(width.is_none(), |d| d.w_full()).child(button);
    if width.is_none() {
        root = root.child(width_probe(id, scope.clone()));
    }
    if !open {
        return root;
    }
    let floor = width
        .unwrap_or_else(|| scope.registry().and_then(|reg| reg.borrow().select_w.get(id).copied()).unwrap_or(200.));
    let mut panel = menu_panel(ElementId::from((ElementId::from(id), "menu")), p, on_toggle.clone())
        .min_w(px(floor.max(180.)))
        .max_w(px(400.));
    // The panel only exists while open, so this runs each time it appears.
    for (i, item) in options.iter().enumerate() {
        panel = panel.child(select_row(
            id,
            p,
            i,
            item,
            selected(i),
            close_on_pick,
            &on_pick,
            close_on_pick.then_some(&on_toggle),
        ));
    }
    root.child(float(ElementId::from((ElementId::from(id), "menu")), look.height() + 6., panel))
}
