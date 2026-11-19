use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::motion::{Motion, SpringCurve, ease};
use super::scope::{self, ScopeHandle};

/// Returns `(from, to, active, key)`. The animation runs from where the value was when the target changed, so an
/// interrupted move continues smoothly; `active` is false once it has settled, so a control drawn again later does
/// not replay it. A handle whose scope has expired settles at once and reinserts nothing.
pub(super) fn tween_to_at(scope: &ScopeHandle, id: &ElementId, target: f32, dur: Duration) -> (f32, f32, bool, usize) {
    let (from, to, start, key) = retarget(scope, id, target, dur, Instant::now(), ease);
    (from, to, start.elapsed() < dur, key)
}

fn retarget(
    scope: &ScopeHandle,
    id: &ElementId,
    target: f32,
    dur: Duration,
    now: Instant,
    curve: fn(f32) -> f32,
) -> (f32, f32, Instant, usize) {
    let Some(reg) = scope.registry() else { return (target, target, now - dur, 0) };
    let mut m = reg.borrow_mut();
    let key = id.clone();
    let e = m.tweens.entry(key).or_insert((target, target, now - dur, 0));
    if (e.1 - target).abs() > f32::EPSILON {
        let progress = (now.duration_since(e.2).as_secs_f32() / dur.as_secs_f32()).clamp(0., 1.);
        let current = e.0 + (e.1 - e.0) * curve(progress);
        *e = (current, target, now, e.3 + 1);
    }
    *e
}

/// Keeps the interactive element that uses the fade mounted.
pub fn fade_value(id: &ElementId, target: f32, ms: u64) -> (f32, bool) {
    fn ease_out(t: f32) -> f32 {
        1. - (1. - t).powi(2)
    }

    let scope = scope::motion_scope();
    let dur = Duration::from_millis(ms);
    let now = Instant::now();
    let (from, to, start, _) = retarget(&scope, id, target, dur, now, ease_out);
    let elapsed = now.duration_since(start);
    if elapsed >= dur {
        return (to, false);
    }
    let progress = elapsed.as_secs_f32() / dur.as_secs_f32();
    (from + (to - from) * ease_out(progress), true)
}

/// For visual layers only: animating re-creates the element, which would reset the hover and press state of an
/// interactive one.
pub fn tweened<E: IntoElement + 'static>(
    id: ElementId,
    target: f32,
    ms: u64,
    build: impl Fn(f32) -> E + 'static,
) -> AnyElement {
    tweened_at(scope::motion_scope(), id, target, ms, build)
}

/// For `build` closures that run during animation frames, when the ambient scope is no longer the one the element
/// was built under. Every build runs with `scope` restored, so nested scoped helpers keep hitting this scope's
/// registry.
pub fn tweened_at<E: IntoElement + 'static>(
    scope: ScopeHandle,
    id: ElementId,
    target: f32,
    ms: u64,
    build: impl Fn(f32) -> E + 'static,
) -> AnyElement {
    let dur = Duration::from_millis(ms);
    let (from, to, active, key) = tween_to_at(&scope, &id, target, dur);
    let render = move |v: f32| scope::with_motion_scope(scope.clone(), || build(v));
    if !active {
        return render(to).into_any_element();
    }
    render(from)
        .with_animation((id, SharedString::from(format!("tween-{key}"))), Animation::new(dur), move |_, t| {
            render(from + (to - from) * ease(t))
        })
        .into_any_element()
}

/// [`tweened`] that hands `build` the linear progress from the drawn value to the target, for callers that shape
/// the motion themselves (a spring, a lead and a trail).
pub fn tweened_linear<E: IntoElement + 'static>(
    id: ElementId,
    target: f32,
    ms: u64,
    build: impl Fn(f32) -> E + 'static,
) -> AnyElement {
    let scope = scope::motion_scope();
    let dur = Duration::from_millis(ms);
    let (from, to, active, key) = tween_to_at(&scope, &id, target, dur);
    let render = move |v: f32| scope::with_motion_scope(scope.clone(), || build(v));
    if !active {
        return render(to).into_any_element();
    }
    render(from)
        .with_animation((id, SharedString::from(format!("linear-{key}"))), Animation::new(dur), move |_, t| {
            render(from + (to - from) * t)
        })
        .into_any_element()
}

/// Under Reduce Motion it is a plain ease without overshoot.
pub fn spring(t: f32, motion: Motion) -> f32 {
    let t = t.clamp(0., 1.);
    if motion.is_reduced() {
        return ease(t);
    }
    if t >= 1. {
        return 1.;
    }
    1. - SpringCurve::GLASS.step(1., 0., t).0
}

/// 0 once the latest flip has settled. GPUI restarts an animation whenever its element reappears, so a control drawn
/// again later must not replay a transition that already happened.
pub(super) fn count_flips(id: &ElementId, value: bool, animation: Duration) -> usize {
    let Some(reg) = scope::ambient_registry() else { return 0 };
    let mut m = reg.borrow_mut();
    let entry = m.flips.entry(id.clone()).or_insert((value, 0, Instant::now()));
    if entry.0 != value {
        *entry = (value, entry.1 + 1, Instant::now());
    }
    if entry.2.elapsed() < animation { entry.1 } else { 0 }
}

pub(super) const RING_MS: u64 = 180;

#[cfg(test)]
mod tests {
    use super::{Duration, ElementId, count_flips, scope};
    use crate::ui::scope::MotionScope;

    #[test]
    fn a_settled_flip_is_not_replayed_when_the_control_is_drawn_again() {
        let scope = MotionScope::new();
        scope::with_motion_scope(scope.handle(), || {
            let id = ElementId::from("flip-test");
            let window = Duration::from_millis(40);
            assert_eq!(count_flips(&id, false, window), 0, "first draw never animates");
            assert_eq!(count_flips(&id, true, window), 1, "a change animates");
            assert_eq!(count_flips(&id, true, window), 1, "...for as long as the animation runs");
            assert_eq!(
                count_flips(&id, true, Duration::ZERO),
                0,
                "later draws (another page, another window) are static"
            );
            assert_eq!(count_flips(&id, false, window), 2, "the next change animates again");
        });
    }
}
