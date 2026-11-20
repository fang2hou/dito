use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::motion::Motion;
use super::scope::{self, ScopeHandle};
use super::tween;

/// Long enough for the spring to settle.
pub const GLASS_MS: u64 = 460;
const GLASS_STRETCH_DELAY: f32 = 0.14;

/// The edge the plate travels toward leads on the spring and the other follows a beat later, so the plate stretches
/// in flight and snaps back as it arrives.
fn glass_edges(from: (f32, f32), to: (f32, f32), t: f32, motion: Motion) -> (f32, f32) {
    let forward = (to.0 + to.1) >= (from.0 + from.1);
    let lead = tween::spring(t, motion);
    let trail = if motion.is_reduced() {
        lead
    } else {
        tween::spring((t - GLASS_STRETCH_DELAY) / (1. - GLASS_STRETCH_DELAY), Motion::Full)
    };
    let (ta, tb) = if forward { (trail, lead) } else { (lead, trail) };
    let a = from.0 + (to.0 - from.0) * ta;
    let b = from.1 + (to.1 - from.1) * tb;
    (a, b.max(a + 1.))
}

/// A transition retains its last rendered edges instead of estimating them with a second clock: layout can delay
/// the animation element's first frame, and an interruption must continue from what the user saw.
pub(super) struct PlateMotion {
    from: (f32, f32),
    to: (f32, f32),
    rendered: (f32, f32),
    progress: f32,
    pub(super) key: usize,
}

impl PlateMotion {
    pub(super) fn settled(target: (f32, f32)) -> Self {
        Self { from: target, to: target, rendered: target, progress: 1., key: 0 }
    }

    pub(super) fn retarget(&mut self, target: (f32, f32)) {
        if (self.to.0 - target.0).abs() > 0.01 || (self.to.1 - target.1).abs() > 0.01 {
            self.from = self.rendered;
            self.to = target;
            self.progress = 0.;
            self.key += 1;
        }
    }

    fn record_frame(&mut self, key: usize, progress: f32, edges: (f32, f32)) {
        // Another layer or an obsolete frame must not rewind the transition.
        if self.key == key && progress >= self.progress {
            self.rendered = edges;
            self.progress = progress;
        }
    }
}

/// A new target mid-flight continues from where the plate currently is. The plate and the labels that follow it
/// animate from one [`PlateState`], so they share one clock.
pub struct PlateState {
    id: ElementId,
    from: (f32, f32),
    to: (f32, f32),
    pub(super) active: bool,
    key: usize,
    motion: Motion,
    /// The scope the plate was built under; builds during deferred animation frames restore it.
    scope: ScopeHandle,
}

pub fn plate_state(id: ElementId, target: (f32, f32)) -> PlateState {
    let scope = scope::motion_scope();
    let Some(reg) = scope.registry() else {
        // No live scope (none set, or the owner was reset): draw statically at the target.
        return PlateState { id, from: target, to: target, active: false, key: 0, motion: Motion::Full, scope };
    };
    let motion = Motion::system();
    let (from, to, active, key) = {
        let mut m = reg.borrow_mut();
        let e = m.plates.entry(id.clone()).or_insert_with(|| PlateMotion::settled(target));
        e.retarget(target);
        (e.from, e.to, e.progress < 1., e.key)
    };
    PlateState { id, from, to, active, key, motion, scope }
}

impl PlateState {
    /// `energy` is 0 at rest and peaks mid-flight. Layers of one plate run on the same clock; each build restores the
    /// plate's scope.
    pub fn animate<E: IntoElement + 'static>(
        &self,
        layer: &str,
        build: impl Fn(f32, f32, f32) -> E + 'static,
    ) -> AnyElement {
        let scope = self.scope.clone();
        let render = move |a: f32, b: f32, energy: f32| scope::with_motion_scope(scope.clone(), || build(a, b, energy));
        if !self.active {
            return render(self.to.0, self.to.1, 0.).into_any_element();
        }
        let (from, to, motion) = (self.from, self.to, self.motion);
        let (scope, id, key) = (self.scope.clone(), self.id.clone(), self.key);
        render(from.0, from.1, 0.)
            .with_animation(
                (self.id.clone(), SharedString::from(format!("{layer}-{}", self.key))),
                Animation::new(Duration::from_millis(GLASS_MS)),
                move |_, t| {
                    let (a, b) = glass_edges(from, to, t, motion);
                    if let Some(reg) = scope.registry()
                        && let Some(plate) = reg.borrow_mut().plates.get_mut(&id)
                    {
                        plate.record_frame(key, t, (a, b));
                    }
                    let energy = if motion.is_reduced() { 0. } else { (4. * t * (1. - t)).clamp(0., 1.) };
                    render(a, b, energy)
                },
            )
            .into_any_element()
    }
}

pub fn coverage(a: f32, b: f32, lo: f32, hi: f32) -> f32 {
    ((b.min(hi) - a.max(lo)) / (hi - lo)).clamp(0., 1.)
}

#[cfg(test)]
mod tests {
    use super::{Motion, PlateMotion, glass_edges};

    #[test]
    fn rapid_navigation_continues_from_the_last_rendered_frame() {
        let mut plate = PlateMotion::settled((0., 44.));
        for _ in 0..100 {
            for row in [0, 1, 1, 0, 0, 1, 1, 0, 1, 2, 2, 2, 1] {
                let target = (row as f32 * 48., row as f32 * 48. + 44.);
                let (previous, key, same) = (plate.rendered, plate.key, plate.to == target);
                plate.retarget(target);
                if same {
                    assert_eq!(plate.key, key, "clicking the selected row preserves its animation");
                    assert_eq!(plate.rendered, previous);
                } else {
                    assert_eq!(plate.from, previous, "an interruption cannot jump ahead of the displayed frame");
                    assert_eq!(plate.progress, 0., "each new target gets a full transition");
                    assert_eq!(plate.key, key + 1);
                }
                let edges = glass_edges(plate.from, plate.to, 0.28, Motion::Full);
                plate.record_frame(plate.key, 0.28, edges);
            }
        }
    }

    #[test]
    fn a_delayed_first_frame_and_obsolete_callbacks_cannot_advance_a_new_transition() {
        let mut plate = PlateMotion::settled((0., 44.));
        plate.retarget((48., 92.));
        // Until the renderer supplies a frame, layout delay cannot advance the plate.
        let old_key = plate.key;
        plate.retarget((96., 140.));
        assert_eq!(plate.from, (0., 44.));
        plate.record_frame(old_key, 1., (48., 92.));
        assert_eq!(plate.rendered, (0., 44.));
        let halfway = glass_edges(plate.from, plate.to, 0.5, Motion::Full);
        plate.record_frame(plate.key, 0.5, halfway);
        plate.record_frame(plate.key, 0.1, (0., 44.));
        assert_eq!(plate.rendered, halfway, "layers cannot rewind one another");
        plate.record_frame(plate.key, 1., plate.to);
        assert_eq!(plate.rendered, plate.to);
        assert_eq!(plate.progress, 1.);
    }
}
