mod motion;

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::time::Instant;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::hover::is_hovered_in;
use super::primitives::white;
use super::scope::{self, ScopeHandle, motion_scope};
use super::tip_bubble::{TIP_BUBBLE_H, TipColors, paint_shared, tip_bubble};
use super::tween::tweened_at;
use crate::theme::Palette;

use motion::{Geometry, TooltipMotion};

const TIP_GAP: f32 = 6.;
const TIP_RISE: f32 = 6.;
const TIP_MS: u64 = 150;
const TIP_FROM: f32 = 0.8;

struct Anchor {
    scope: ScopeHandle,
    id: ElementId,
    epoch: u64,
    explicit: Option<bool>,
    frame: u64,
    label: SharedString,
    bounds: Bounds<Pixels>,
    above: bool,
    colors: TipColors,
    suppressed: bool,
}

impl Anchor {
    fn requested(&self) -> bool {
        !self.suppressed && self.active()
    }

    fn active(&self) -> bool {
        self.scope.registry().is_some_and(|registry| {
            registry.borrow().tip_epoch == self.epoch
                && self.explicit.unwrap_or_else(|| is_hovered_in(&self.scope, &self.id))
        })
    }
}

#[derive(Default)]
struct WindowTips {
    anchors: Vec<Weak<RefCell<Anchor>>>,
    selected: Option<Rc<RefCell<Anchor>>>,
    motion: Option<TooltipMotion>,
    frame: u64,
    key_listener: Option<Subscription>,
}

/// Window-owned state, independent of the current view's element-id namespace.
fn window_tips(window: &mut Window) -> Rc<RefCell<WindowTips>> {
    window.with_element_state(&GlobalElementId::default(), |state: Option<Rc<RefCell<WindowTips>>>, _| {
        let state = state.unwrap_or_default();
        (state.clone(), state)
    })
}

/// A non-interactive anchor. Its lease expires when the trigger leaves the element tree.
fn anchor_marker(anchor: Anchor) -> impl IntoElement {
    let id = anchor.id.clone();
    canvas(
        move |bounds, window, _| {
            let tips = window_tips(window);
            let mut anchor = anchor;
            anchor.bounds = bounds;
            anchor.frame = tips.borrow().frame;
            window.with_global_id(ElementId::from((id, "tooltip-anchor")), |id, window| {
                window.with_element_state(id, |existing: Option<Rc<RefCell<Anchor>>>, _| {
                    let lease = if let Some(lease) = existing {
                        anchor.suppressed = lease.borrow().suppressed && anchor.active();
                        *lease.borrow_mut() = anchor;
                        lease
                    } else {
                        let lease = Rc::new(RefCell::new(anchor));
                        tips.borrow_mut().anchors.push(Rc::downgrade(&lease));
                        lease
                    };
                    ((), lease)
                });
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

/// Anchors only register bounds during prepaint, so all hovered targets are known before this deferred layer chooses
/// and paints a single bubble.
fn tooltip_layer(slot: Rc<RefCell<Weak<RefCell<WindowTips>>>>) -> impl IntoElement {
    deferred(
        canvas(
            move |_, window, cx| {
                let tips = window_tips(window);
                if tips.borrow().key_listener.is_none() {
                    let weak = Rc::downgrade(&tips);
                    let owner = window.window_handle();
                    let listener = cx.intercept_keystrokes(move |event, window, _| {
                        if window.window_handle() == owner && event.keystroke.key == "escape" {
                            dismiss_window_tips(&weak, window);
                        }
                    });
                    tips.borrow_mut().key_listener = Some(listener);
                }
                *slot.borrow_mut() = Rc::downgrade(&tips);
                tips
            },
            |_, tips, window, cx| tips.borrow_mut().paint(window, cx),
        )
        .absolute()
        .inset_0(),
    )
    .with_priority(2)
}

/// Attach dismissal to the owning root without consuming the event's normal action.
pub fn tooltip_root(root: Div) -> Div {
    let slot = Rc::new(RefCell::new(Weak::new()));
    let scroll_slot = slot.clone();
    root.capture_any_mouse_down(|_, window, _| window.refresh())
        .on_scroll(move |_, window, _| dismiss_window_tips(&scroll_slot.borrow(), window))
        .child(tooltip_layer(slot))
}

fn dismiss_window_tips(tips: &Weak<RefCell<WindowTips>>, window: &mut Window) {
    let Some(tips) = tips.upgrade() else { return };
    let mut tips = tips.borrow_mut();
    for anchor in tips.anchors.iter().filter_map(Weak::upgrade) {
        let mut anchor = anchor.borrow_mut();
        anchor.suppressed = true;
        if let Some(registry) = anchor.scope.registry() {
            let mut registry = registry.borrow_mut();
            registry.hovered.remove(&anchor.id);
            registry.tip_epoch += 1;
        }
    }
    if let Some(motion) = &mut tips.motion {
        motion.dismiss(Instant::now());
    }
    window.refresh();
}

impl WindowTips {
    fn paint(&mut self, window: &mut Window, cx: &mut App) {
        let frame = self.frame;
        self.frame = self.frame.wrapping_add(1);
        self.anchors.retain(|anchor| anchor.strong_count() > 0);
        if self.selected.as_ref().is_some_and(|anchor| {
            let anchor = anchor.borrow();
            anchor.scope.registry().is_none_or(|registry| registry.borrow().tip_epoch != anchor.epoch)
        }) && let Some(motion) = &mut self.motion
        {
            motion.dismiss(Instant::now());
        }
        let target = self.anchors.iter().rev().filter_map(Weak::upgrade).find(|anchor| {
            let anchor = anchor.borrow();
            anchor.frame == frame && anchor.requested()
        });
        let now = Instant::now();
        let reduced = dito_platform::system::reduce_motion();
        if let Some(target) = &target {
            let anchor = target.borrow();
            let width = (super::text_width(&anchor.label, 12., FontWeight::MEDIUM) + 21.)
                .min(f32::from(window.viewport_size().width) - 16.)
                .max(22.);
            let center = (f32::from(anchor.bounds.origin.x + anchor.bounds.size.width * 0.5)).clamp(
                8. + width * 0.5,
                (f32::from(window.viewport_size().width) - 8. - width * 0.5).max(8. + width * 0.5),
            );
            let mut bottom = f32::from(anchor.bounds.bottom_left().y);
            if anchor.above {
                bottom = f32::from(anchor.bounds.origin.y) - TIP_GAP;
                if bottom < TIP_BUBBLE_H + 8. {
                    bottom = f32::from(anchor.bounds.bottom_left().y) + TIP_GAP + TIP_BUBBLE_H;
                }
            }
            bottom =
                bottom.clamp(TIP_BUBBLE_H + 8., f32::from(window.viewport_size().height).max(TIP_BUBBLE_H + 16.) - 8.);
            let geometry = Geometry { center, bottom, width };
            if self.motion.as_ref().is_none_or(|motion| {
                motion.alpha(now) < 0.001 && now.duration_since(motion.last_seen) > motion::HANDOFF
            }) {
                self.motion = Some(TooltipMotion::new(geometry, anchor.label.clone(), now));
            }
            self.motion.as_mut().unwrap().target(geometry, anchor.label.clone(), now, reduced);
            self.selected = Some(target.clone());
        } else if let Some(motion) = &mut self.motion {
            motion.leave(now);
        }
        let Some(motion) = &self.motion else { return };
        let Some(anchor) = self.selected.as_ref() else { return };
        let anchor = anchor.borrow();
        let alpha = motion.alpha(now);
        if alpha > 0.001 {
            let mut geometry = motion.geometry(now);
            geometry.width = geometry.width.min((f32::from(window.viewport_size().width) - 16.).max(22.));
            geometry.center = geometry.center.clamp(
                8. + geometry.width * 0.5,
                (f32::from(window.viewport_size().width) - 8. - geometry.width * 0.5).max(8. + geometry.width * 0.5),
            );
            let scale = if reduced { 1. } else { TIP_FROM + (1. - TIP_FROM) * alpha };
            let bounds = Bounds::new(
                point(
                    px(geometry.center - geometry.width * 0.5),
                    px(geometry.bottom - TIP_BUBBLE_H + if reduced { 0. } else { (1. - alpha) * TIP_RISE }),
                ),
                size(px(geometry.width), px(TIP_BUBBLE_H)),
            );
            paint_shared(anchor.colors, motion.labels(now), bounds, scale, alpha, window, cx);
        }
        // Refresh the whole window during handoffs, including cached sibling views' anchors.
        if motion.moving(now)
            || (target.is_none() && alpha > 0.001)
            || target.as_ref().is_some_and(|anchor| anchor.borrow().frame != frame)
        {
            window.on_next_frame(|window, _| window.refresh());
        }
    }
}

pub fn tip_above(p: &Palette, id: &ElementId) -> AnyElement {
    let scope = motion_scope();
    let (label, epoch) = scope
        .registry()
        .map(|registry| {
            let registry = registry.borrow();
            (registry.tips.get(id).cloned().unwrap_or_default(), registry.tip_epoch)
        })
        .unwrap_or_default();
    let (bg, fg) = if p.dark { (p.side, white()) } else { (p.hud_bg, p.text) };
    anchor_marker(Anchor {
        scope,
        id: id.clone(),
        epoch,
        explicit: None,
        frame: 0,
        label,
        bounds: Bounds::default(),
        above: true,
        colors: TipColors::new(p, bg, fg),
        suppressed: false,
    })
    .into_any_element()
}

/// Explicitly driven HUD tips share the same window-owned bubble as hover-driven tips.
pub fn tip_pop_with(p: &Palette, id: ElementId, label: SharedString, on: bool, bg: Hsla, fg: Hsla) -> AnyElement {
    let scope = motion_scope();
    let epoch = scope.registry().map_or(0, |registry| registry.borrow().tip_epoch);
    let width = super::text_width(&label, 12., FontWeight::MEDIUM) + 21.;
    div()
        .relative()
        .flex_none()
        .w(px(width))
        .h(px(TIP_BUBBLE_H))
        .child(anchor_marker(Anchor {
            scope,
            id,
            epoch,
            explicit: Some(on),
            frame: 0,
            label,
            bounds: Bounds::default(),
            above: false,
            colors: TipColors::new(p, bg, fg),
            suppressed: false,
        }))
        .into_any_element()
}

pub fn tip_pop(p: &Palette, id: ElementId, label: SharedString, on: bool) -> AnyElement {
    let scope = motion_scope();
    let p = *p;
    let reduced = dito_platform::system::reduce_motion();
    let (bg, fg) = if p.dark { (p.side, white()) } else { (p.hud_bg, p.text) };
    tweened_at(scope, id, if on { 1. } else { 0. }, TIP_MS, move |v| {
        let v = 1. - (1. - v).powi(3);
        let k = if reduced { 1. } else { TIP_FROM + (1. - TIP_FROM) * v };
        div().mt(px(if reduced { 0. } else { (1. - v) * TIP_RISE })).child(tip_bubble(&p, &label, k, v, bg, fg))
    })
}

/// Called during the control's build, so the label always matches the rendered language.
pub fn tip_label(id: ElementId, label: SharedString) {
    if let Some(reg) = scope::ambient_registry() {
        reg.borrow_mut().tips.insert(id, label);
    }
}

/// Dismiss hover tips on scroll without retiring the page's other motion state.
pub fn dismiss_tips(scope: &super::scope::ScopeHandle) {
    if let Some(reg) = scope.registry() {
        let mut reg = reg.borrow_mut();
        reg.tip_epoch += 1;
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
