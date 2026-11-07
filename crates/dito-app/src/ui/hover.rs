use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::primitives::mix;
use super::scope::{self, ScopeHandle};
use super::tween::{self, tweened};
use super::typography::text;
use crate::theme::Palette;

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

/// Put on an interactive element to make [`is_hovered`] work for `id`.
pub fn hover_tracker(id: ElementId) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    // Captured at build time: hover events fire outside the render that scoped the element.
    let (scope, key) = (scope::motion_scope(), id);
    move |over, window, _| {
        let Some(reg) = scope.registry() else { return };
        let changed = reg.borrow_mut().hovered.insert(key.clone(), *over).unwrap_or(false) != *over;
        if changed {
            window.refresh();
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

/// Pretend the pointer is over `id` (screenshots), in the scope the id actually renders under.
#[cfg(feature = "screenshots")]
pub fn force_hover(scope: &ScopeHandle, id: ElementId, over: bool) {
    if let Some(reg) = scope.registry() {
        reg.borrow_mut().hovered.insert(id, over);
    }
}

/// The hover pop of the HUD's in-flow content: it rises and fades in when it appears and is not drawn once the
/// pointer leaves. The value lives in the registry, so unmounting on leave still settles it to 0 and every
/// re-appearance pops. Unlike [`tweened_at`] the content is built once per render and only the wrapper is
/// restyled each frame, which keeps interactive state (hover, presses) intact.
pub fn hover_pop<E: IntoElement + 'static>(id: ElementId, on: bool, build: impl FnOnce() -> E) -> Option<AnyElement> {
    const MS: u64 = 150;
    const RISE: f32 = 6.;
    let dur = Duration::from_millis(MS);
    let scope = scope::motion_scope();
    let (from, to, active, bump) = tween::tween_to_at(&scope, &id, if on { 1. } else { 0. }, dur);
    if !on {
        return None;
    }
    let el = build();
    if !active {
        return Some(el.into_any_element());
    }
    let reduced = dito_platform::system::reduce_motion();
    Some(
        div()
            .child(el)
            .with_animation((id, SharedString::from(format!("pop-{bump}"))), Animation::new(dur), move |el, t| {
                let v = from + (to - from) * (1. - (1. - t.clamp(0., 1.)).powi(3));
                let rise = if reduced { 0. } else { (1. - v) * RISE };
                el.mt(px(rise)).opacity(v)
            })
            .into_any_element(),
    )
}

/// [`hover_pop`] that also grows the content out of its bottom edge, from `from` to full size. GPUI has no
/// transform for a plain element, so `build(k)` redraws the content at scale `k` on every frame of the pop and
/// at `k = 1` once settled. The rise is a relative offset, so the layout never moves.
pub fn hover_pop_scaled<E: IntoElement + 'static>(
    id: ElementId,
    on: bool,
    from: f32,
    build: impl Fn(f32) -> E + 'static,
) -> Option<AnyElement> {
    const MS: u64 = 150;
    const RISE: f32 = 6.;
    let scope = scope::motion_scope();
    if !on {
        tween::tween_to_at(&scope, &id, 0., Duration::from_millis(MS));
        return None;
    }
    let reduced = dito_platform::system::reduce_motion();
    Some(tween::tweened_at(scope, id, 1., MS, move |v| {
        let e = 1. - (1. - v).powi(3);
        let (k, rise) = if reduced { (1., 0.) } else { (from + (1. - from) * e, (1. - e) * RISE) };
        div().relative().top(px(rise)).opacity(e).child(build(k))
    }))
}

/// A rounded fill laid over an element whose opacity follows hover: `color` fades in when `id` is hovered.
pub fn hover_fill(id: &ElementId, radius: Option<f32>, color: Hsla) -> AnyElement {
    hover_fill_if(id, radius, color, true)
}

/// Like [`hover_fill`], but only while `enabled`; turning it off (e.g. the item became the selected one) fades the
/// fill out instead of dropping it, so nothing blinks.
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

/// Label of a choice card: medium weight always, brightness eased between idle, hovered and chosen.
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
