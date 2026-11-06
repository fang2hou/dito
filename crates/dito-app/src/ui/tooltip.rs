use gpui_kit::prelude::*;
use gpui_kit::*;

use super::hover::is_hovered;
use super::primitives::white;
use super::scope::{self, motion_scope};
use super::tip_bubble::{TIP_BUBBLE_H, tip_bubble};
use super::tween::tweened_at;
use crate::theme::Palette;

/// Gap between a tooltip and its anchor, how far it still has to rise when it starts appearing,
/// how long that takes, and the size it pops out of the trigger at.
const TIP_GAP: f32 = 6.;
const TIP_RISE: f32 = 6.;
const TIP_MS: u64 = 150;
const TIP_FROM: f32 = 0.8;

/// The tooltip for a trigger: put it next to the trigger's content and pass the trigger's
/// height. While hovered, one bubble pops out of the trigger — growing from 0.8 to full size
/// as it rises the last few pixels and fades in (150 ms, ease-out) — and reverses the motion when
/// the hover leaves. The positioned box is a plain static element (an animated one re-lays out
/// against the wrong ancestor); only its inner child tweens. Centring does not measure the
/// text: the box is symmetric about the trigger's centre (non-zero insets — a zero `left` lays
/// out as if unset), so a longer translation cannot drift it off-centre.
pub fn tip_above(p: &Palette, id: &ElementId) -> AnyElement {
    let on = is_hovered(id);
    let label = scope::ambient_registry().and_then(|reg| reg.borrow().tips.get(id).cloned()).unwrap_or_default();
    // Priority 2: above the dropdown panels' floating layer (priority 1), so a tip anchored
    // inside an open menu is never painted underneath the menu itself.
    deferred(
        div()
            .absolute()
            .top(px(-(TIP_BUBBLE_H + TIP_GAP)))
            .left(px(0.5))
            .right(px(0.5))
            .flex()
            .justify_center()
            .child(tip_pop(p, ElementId::from((id.clone(), "tip")), label, on)),
    )
    .with_priority(2)
    .into_any_element()
}

/// The tweened inner of a tooltip, shared by the hover tips ([`tip_above`]) and the
/// shortcut-recording hint. `on` decides whether it shows: a tween only plays when its target
/// *changes*, so while off the value is held at 0 (an element that first appears already at
/// its target would render statically) and the 0→1 flip when it turns on is the pop-in —
/// growing from [`TIP_FROM`] to full size while rising and fading in, ease-out. Turning off
/// reverses the same tween and retains the bubble until its opacity reaches zero.
pub fn tip_pop(p: &Palette, tween_id: ElementId, label: SharedString, on: bool) -> AnyElement {
    let (bg, fg) = if p.dark { (p.side, white()) } else { (p.hud_bg, p.text) };
    tip_pop_with(p, tween_id, label, on, bg, fg)
}

/// [`tip_pop`] with the bubble chrome stated by the caller — the HUD passes its own surface
/// colors, which differ from the settings surface in dark mode.
pub fn tip_pop_with(p: &Palette, tween_id: ElementId, label: SharedString, on: bool, bg: Hsla, fg: Hsla) -> AnyElement {
    let scope = motion_scope();
    let p = *p;
    let reduced = dito_platform::system::reduce_motion();
    tweened_at(scope, tween_id, if on { 1. } else { 0. }, TIP_MS, move |v| {
        if v <= 0.01 {
            return div().into_any_element();
        }
        let v = 1. - (1. - v).powi(3);
        let k = if reduced { 1. } else { TIP_FROM + (1. - TIP_FROM) * v };
        div()
            .mt(px(if reduced { 0. } else { (1. - v) * TIP_RISE }))
            .child(tip_bubble(&p, &label, k, v, bg, fg))
            .into_any_element()
    })
}

/// Remember `label` as the tooltip text for the control `id` (see [`tip_above`]). Called during
/// the control's build, so the label always matches the rendered language.
pub fn tip_label(id: ElementId, label: SharedString) {
    if let Some(reg) = scope::ambient_registry() {
        reg.borrow_mut().tips.insert(id, label);
    }
}

/// Dismiss hover tips on scroll without retiring the page's other motion state.
pub fn dismiss_tips(scope: &super::scope::ScopeHandle) {
    if let Some(reg) = scope.registry() {
        let mut reg = reg.borrow_mut();
        let keys: Vec<_> = reg.tips.keys().cloned().collect();
        for key in keys {
            reg.hovered.remove(&key);
            reg.tweens.remove(&ElementId::from((key, "tip")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{dismiss_tips, tip_label};
    use crate::ui::{MotionScope, is_hovered_in, with_motion_scope};
    use gpui_kit::ElementId;

    #[test]
    fn dismiss_uses_the_captured_scope_and_preserves_other_hover_state() {
        let mut owner = MotionScope::new();
        let scope = owner.handle();
        let tip = ElementId::from("model-action");
        let other = ElementId::from("unrelated-control");
        with_motion_scope(scope.clone(), || tip_label(tip.clone(), "Download".into()));
        {
            let reg = scope.registry().unwrap();
            let mut reg = reg.borrow_mut();
            reg.hovered.insert(tip.clone(), true);
            reg.hovered.insert(other.clone(), true);
        }
        // Scroll callbacks run outside the render's ambient scope.
        dismiss_tips(&scope);
        assert!(!is_hovered_in(&scope, &tip));
        assert!(is_hovered_in(&scope, &other));
        owner.reset();
        dismiss_tips(&scope);
        assert!(scope.expired());
    }
}
