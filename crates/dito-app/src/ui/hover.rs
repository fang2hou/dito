use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::motion::ease;
use super::primitives::mix;
use super::scope::{self, ScopeHandle};
use super::tween::{self, tweened};
use super::typography::text;
use crate::ui::Palette;

pub const HOVER_MS: u64 = 160;

/// Forget the hover remembered for one control that stops being drawn while others stay, so it cannot come back
/// looking hovered.
pub fn reset_hover(id: &ElementId) {
    reset_hover_in(scope::motion_scope(), id);
}

/// [`reset_hover`] against one explicit scope, for click handlers: they run after the build, when the ambient scope
/// is detached, and need the scope the control was built under.
pub fn reset_hover_in(scope: ScopeHandle, id: &ElementId) {
    let Some(reg) = scope.registry() else { return };
    let mut reg = reg.borrow_mut();
    reg.hovered.remove(id);
    reg.tweens.remove(&ElementId::from((id.clone(), "hover-fill")));
}

pub fn hover_tracker(id: ElementId) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    // Captured at build time: hover events fire outside the render that scoped the element.
    let (scope, key) = (scope::motion_scope(), id);
    if let Some(reg) = scope.registry() {
        let mut reg = reg.borrow_mut();
        let build = reg.build;
        reg.hover_seen.insert(key.clone(), build);
    }
    move |over, window, cx| {
        let Some(reg) = scope.registry() else { return };
        let (changed, owner) = {
            let mut reg = reg.borrow_mut();
            (reg.hovered.insert(key.clone(), *over).unwrap_or(false) != *over, reg.owner)
        };
        if changed {
            match owner {
                Some(owner) => cx.notify(owner),
                None => window.refresh(),
            }
        }
    }
}

pub fn is_hovered(id: &ElementId) -> bool {
    is_hovered_in(&scope::motion_scope(), id)
}

/// [`is_hovered`] against one explicit scope, for readers outside the build (e.g. the screenshot
/// harness probing the shell).
pub fn is_hovered_in(scope: &ScopeHandle, id: &ElementId) -> bool {
    scope.registry().is_some_and(|r| r.borrow().hovered.get(id).copied().unwrap_or(false))
}

/// `scope` must be the one `id` renders under.
#[cfg(feature = "screenshots")]
pub fn force_hover(scope: &ScopeHandle, id: ElementId, over: bool) {
    if let Some(reg) = scope.registry() {
        reg.borrow_mut().hovered.insert(id, over);
    }
}

/// Keep the same interactive child mounted through entrance, exit and interrupted re-entry.
pub fn hover_pop<E: IntoElement + 'static>(id: ElementId, on: bool, build: impl FnOnce() -> E) -> Option<AnyElement> {
    const MS: u64 = 150;
    const RISE: f32 = 6.;
    let dur = Duration::from_millis(MS);
    let scope = scope::motion_scope();
    if scope.registry().is_some_and(|reg| !reg.borrow().tweens.contains_key(&id)) {
        tween::tween_to_at(&scope, &id, 0., dur);
    }
    let (from, to, active, bump) = tween::tween_to_at(&scope, &id, if on { 1. } else { 0. }, dur);
    if !on && !active {
        return None;
    }
    let el = build();
    if !active {
        return Some(el.into_any_element());
    }
    let reduced = super::motion::reduce_motion();
    Some(
        div()
            .child(el)
            .with_animation((id, SharedString::from(format!("pop-{bump}"))), Animation::new(dur), move |el, t| {
                let v = from + (to - from) * ease(t.clamp(0., 1.));
                let rise = if reduced { 0. } else { (1. - v) * RISE };
                el.relative().top(px(rise)).opacity(v)
            })
            .into_any_element(),
    )
}

/// GPUI has no transform for a plain element, so `build(k)` redraws the content at scale `k` on every frame of the
/// pop and at `k = 1` once settled. The rise is a relative offset, so the layout never moves.
pub fn hover_pop_scaled<E: IntoElement + 'static>(
    id: ElementId,
    on: bool,
    from: f32,
    build: impl Fn(f32) -> E + 'static,
) -> Option<AnyElement> {
    const MS: u64 = 150;
    const RISE: f32 = 6.;
    let scope = scope::motion_scope();
    let dur = Duration::from_millis(MS);
    if scope.registry().is_some_and(|reg| !reg.borrow().tweens.contains_key(&id)) {
        tween::tween_to_at(&scope, &id, 0., dur);
    }
    let (_, _, active, _) = tween::tween_to_at(&scope, &id, f32::from(on), dur);
    if !on && !active {
        return None;
    }
    let reduced = super::motion::reduce_motion();
    Some(tween::tweened_at(scope, id, f32::from(on), MS, move |v| {
        let e = 1. - (1. - v).powi(3);
        let (k, rise) = if reduced { (1., 0.) } else { (from + (1. - from) * e, (1. - e) * RISE) };
        div().relative().top(px(rise)).opacity(e).child(build(k))
    }))
}

pub fn hover_fill(id: &ElementId, radius: Option<f32>, color: Hsla) -> AnyElement {
    hover_fill_if(id, radius, color, true)
}

/// Turning `enabled` off (e.g. the item became the selected one) fades the fill out instead of dropping it, so
/// nothing blinks.
pub fn hover_fill_if(id: &ElementId, radius: Option<f32>, color: Hsla, enabled: bool) -> AnyElement {
    let target = if enabled && is_hovered(id) { 1. } else { 0. };
    tweened(ElementId::from((id.clone(), "hover-fill")), target, HOVER_MS, move |v| {
        let d = div().absolute().inset_0().bg(Hsla { a: color.a * v, ..color });
        match radius {
            Some(r) => d.rounded(px(r)),
            None => d.rounded_full(),
        }
    })
}

/// A full-row fill for a row that touches its card's rounded edge: `top` and `bottom` are the corner radii of the
/// row's own top and bottom edges, because GPUI clips children to a rectangle and would square the card's corners.
pub fn hover_fill_edges(id: &ElementId, top: f32, bottom: f32, color: Hsla) -> AnyElement {
    let target = if is_hovered(id) { 1. } else { 0. };
    tweened(ElementId::from((id.clone(), "hover-fill")), target, HOVER_MS, move |v| {
        div()
            .absolute()
            .inset_0()
            .rounded_tl(px(top))
            .rounded_tr(px(top))
            .rounded_bl(px(bottom))
            .rounded_br(px(bottom))
            .bg(Hsla { a: color.a * v, ..color })
    })
}

pub fn choice_label(id: &ElementId, p: &Palette, label: SharedString, size: f32, on: bool) -> AnyElement {
    let level = if on {
        1.
    } else if is_hovered(id) {
        0.6
    } else {
        0.
    };
    let (text_c, muted_c) = (p.text, p.muted);
    tweened(ElementId::from((id.clone(), "label")), level, HOVER_MS, move |v| {
        text(label.clone(), size, FontWeight::MEDIUM, mix(muted_c, text_c, v)).line_height(relative(1.))
    })
}
