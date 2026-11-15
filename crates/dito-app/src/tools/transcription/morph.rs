//! Retargetable two-axis morph engine for the recording dock surface in `design:tools/recording`.

use std::mem;
use std::time::{Duration, Instant};

use crate::ui::Motion;

/// Why the dock is changing shape: which spring runs, which axis leads, and the Reduce Motion fallback ease.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum MorphKind {
    /// Any capsule-to-capsule change: the glass spring, width leading, height trailing 64 ms later.
    Capsule,
    /// Growing into a taller session shape: the glass spring, width leading, height trailing 64 ms later.
    Expand,
    /// A session shape shrinking: the stiffer collapse spring, height leading, width trailing 64 ms later.
    Collapse,
    /// Pointer down on the start button: both axes ease out together to the pressed size.
    Press,
}

/// The dock's width and height, each axis on its own retargetable motion.
pub(super) struct Morph {
    width: Axis,
    height: Axis,
    target: (f32, f32),
}

impl Morph {
    /// Starts settled at the given size.
    pub(super) fn new(width: f32, height: f32) -> Self {
        Self {
            width: Axis { state: AxisState::Rest(width) },
            height: Axis { state: AxisState::Rest(height) },
            target: (width, height),
        }
    }

    /// Moves both axes toward the given size; a retarget to the current target does nothing.
    pub(super) fn retarget(&mut self, width: f32, height: f32, kind: MorphKind, motion: Motion, now: Instant) {
        if (width, height) == self.target {
            return;
        }
        self.target = (width, height);
        match plan(kind, motion) {
            Plan::Ease { curve, ms } => {
                self.width.begin_eased(width, curve, ms, now);
                self.height.begin_eased(height, curve, ms, now);
            }
            Plan::Spring { lead_width, spec } => {
                let (lead, trail, lead_target, trail_target) = if lead_width {
                    (&mut self.width, &mut self.height, width, height)
                } else {
                    (&mut self.height, &mut self.width, height, width)
                };
                lead.begin_spring(lead_target, spec, now);
                trail.trail_spring(trail_target, spec, now);
            }
        }
    }

    /// The size both axes are currently moving toward.
    pub(super) fn target(&self) -> (f32, f32) {
        self.target
    }

    /// The size to draw at `now`.
    pub(super) fn sample(&self, now: Instant) -> (f32, f32) {
        (self.width.value_at(now), self.height.value_at(now))
    }

    /// True when both axes are within 0.25 pt of the target, under 2 pt per second, past any trailing delay.
    pub(super) fn settled(&self, now: Instant) -> bool {
        self.width.settled_at(now) && self.height.settled_at(now)
    }

    /// Jumps both axes to the target with zero velocity, for screenshots.
    pub(super) fn settle(&mut self) {
        self.width.state = AxisState::Rest(self.target.0);
        self.height.state = AxisState::Rest(self.target.1);
    }
}

/// Named eases shared by axis and fade motions.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Ease {
    Out,
    In,
    InOut,
}

impl Ease {
    fn apply(self, t: f32) -> f32 {
        match self {
            Self::Out => 1. - (1. - t).powi(3),
            Self::In => t * t * t,
            Self::InOut => t * t * (3. - 2. * t),
        }
    }

    /// The analytic derivative of `apply`, for handing a velocity to a spring that interrupts the ease.
    fn slope(self, t: f32) -> f32 {
        match self {
            Self::Out => 3. * (1. - t) * (1. - t),
            Self::In => 3. * t * t,
            Self::InOut => 6. * t * (1. - t),
        }
    }
}

/// An ease's duration, delay, and curve, in milliseconds.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Timing {
    pub(super) duration_ms: f32,
    pub(super) delay_ms: f32,
    pub(super) curve: Ease,
}

impl Timing {
    /// Content entering a morph: 180 ms ease-out after the delay; Reduce Motion uses 150 ms ease-in-out.
    pub(super) fn materialize_in(motion: Motion, delay_ms: f32) -> Self {
        if motion.is_reduced() {
            Self { duration_ms: 150., delay_ms, curve: Ease::InOut }
        } else {
            Self { duration_ms: 180., delay_ms, curve: Ease::Out }
        }
    }

    /// Content leaving a morph: 120 ms ease-in; Reduce Motion uses 150 ms ease-in-out.
    pub(super) fn materialize_out(motion: Motion) -> Self {
        if motion.is_reduced() {
            Self { duration_ms: 150., delay_ms: 0., curve: Ease::InOut }
        } else {
            Self { duration_ms: 120., delay_ms: 0., curve: Ease::In }
        }
    }
}

/// One scalar eased toward a target; a retarget continues from the value currently drawn.
pub(super) struct Fade {
    target: f32,
    step: Option<Step>,
}

struct Step {
    from: f32,
    start: Instant,
    timing: Timing,
}

impl Fade {
    /// Starts at rest at the given value.
    pub(super) fn new(value: f32) -> Self {
        Self { target: value, step: None }
    }

    /// Eases toward `target` per `timing`, holding the drawn value through the delay; a repeat is a no-op.
    pub(super) fn retarget(&mut self, target: f32, timing: Timing, now: Instant) {
        if target == self.target {
            return;
        }
        let from = self.sample(now);
        self.target = target;
        self.step = Some(Step { from, start: now, timing });
    }

    /// The value to draw at `now`: the held start value during the delay, the ease afterwards.
    pub(super) fn sample(&self, now: Instant) -> f32 {
        let Some(step) = &self.step else { return self.target };
        let begin = step.start + duration_from_ms(step.timing.delay_ms);
        if now < begin {
            return step.from;
        }
        eased_value(step.from, self.target, step.timing.curve, begin, duration_from_ms(step.timing.duration_ms), now)
    }

    /// The value the latest retarget asked for.
    pub(super) fn target(&self) -> f32 {
        self.target
    }

    /// True once the delay and the ease have both finished.
    pub(super) fn settled(&self, now: Instant) -> bool {
        match &self.step {
            None => true,
            Some(step) => {
                now >= step.start + duration_from_ms(step.timing.delay_ms) + duration_from_ms(step.timing.duration_ms)
            }
        }
    }

    /// Jumps to the target, for screenshots.
    pub(super) fn settle(&mut self) {
        self.step = None;
    }
}

enum Plan {
    Ease { curve: Ease, ms: f32 },
    Spring { lead_width: bool, spec: SpringSpec },
}

const TRAIL_DELAY: Duration = Duration::from_millis(64);

fn plan(kind: MorphKind, motion: Motion) -> Plan {
    if motion.is_reduced() {
        let ms = match kind {
            MorphKind::Capsule => 120.,
            MorphKind::Expand | MorphKind::Collapse => 200.,
            MorphKind::Press => 90.,
        };
        return Plan::Ease { curve: Ease::InOut, ms };
    }
    match kind {
        MorphKind::Press => Plan::Ease { curve: Ease::Out, ms: 90. },
        MorphKind::Capsule | MorphKind::Expand => Plan::Spring { lead_width: true, spec: glass_spec() },
        MorphKind::Collapse => Plan::Spring { lead_width: false, spec: collapse_spec() },
    }
}

#[derive(Clone, Copy)]
struct SpringSpec {
    zeta: f32,
    omega0: f32,
}

/// The app-wide motion/glass curve (decay 5.5, damped frequency 5.3 over a 460 ms window) as damping and frequency.
fn glass_spec() -> SpringSpec {
    SpringSpec { zeta: 0.72, omega0: (5.5_f32 * 5.5_f32 + 5.3_f32 * 5.3_f32).sqrt() / 0.46 }
}

fn collapse_spec() -> SpringSpec {
    SpringSpec { zeta: 0.82, omega0: 17. }
}

struct Axis {
    state: AxisState,
}

enum AxisState {
    Rest(f32),
    Spring {
        start: Instant,
        target: f32,
        x0: f32,
        v0: f32,
        spec: SpringSpec,
    },
    Eased {
        start: Instant,
        from: f32,
        target: f32,
        dur: Duration,
        curve: Ease,
    },
    /// The carrier motion runs through the delay, then the spring starts from where the carrier is at `until`.
    Trail {
        until: Instant,
        carrier: Box<AxisState>,
        target: f32,
        x0: f32,
        v0: f32,
        spec: SpringSpec,
    },
}

impl Axis {
    fn value_at(&self, now: Instant) -> f32 {
        self.state.value_at(now)
    }

    fn velocity_at(&self, now: Instant) -> f32 {
        self.state.velocity_at(now)
    }

    fn settled_at(&self, now: Instant) -> bool {
        self.state.settled_at(now)
    }

    fn begin_eased(&mut self, target: f32, curve: Ease, ms: f32, now: Instant) {
        let from = self.value_at(now);
        self.state = AxisState::Eased { start: now, from, target, dur: duration_from_ms(ms), curve };
    }

    fn begin_spring(&mut self, target: f32, spec: SpringSpec, now: Instant) {
        let x0 = self.value_at(now) - target;
        let v0 = self.velocity_at(now);
        self.state = AxisState::Spring { start: now, target, x0, v0, spec };
    }

    fn trail_spring(&mut self, target: f32, spec: SpringSpec, now: Instant) {
        self.state.advance(now);
        let until = now + TRAIL_DELAY;
        let x0 = self.value_at(until) - target;
        let v0 = self.velocity_at(until);
        let carrier = Box::new(mem::replace(&mut self.state, AxisState::Rest(target)));
        self.state = AxisState::Trail { until, carrier, target, x0, v0, spec };
    }
}

impl AxisState {
    fn value_at(&self, now: Instant) -> f32 {
        match self {
            Self::Rest(value) => *value,
            Self::Spring { start, target, x0, v0, spec } => {
                target + spring_state(*spec, *x0, *v0, elapsed_since(*start, now)).0
            }
            Self::Eased { start, from, target, dur, curve } => eased_value(*from, *target, *curve, *start, *dur, now),
            Self::Trail { until, carrier, .. } if now < *until => carrier.value_at(now),
            Self::Trail { until, target, x0, v0, spec, .. } => {
                target + spring_state(*spec, *x0, *v0, elapsed_since(*until, now)).0
            }
        }
    }

    fn velocity_at(&self, now: Instant) -> f32 {
        match self {
            Self::Rest(_) => 0.,
            Self::Spring { start, x0, v0, spec, .. } => spring_state(*spec, *x0, *v0, elapsed_since(*start, now)).1,
            Self::Eased { start, from, target, dur, curve } => {
                eased_velocity(*from, *target, *curve, *start, *dur, now)
            }
            Self::Trail { until, carrier, .. } if now < *until => carrier.velocity_at(now),
            Self::Trail { until, x0, v0, spec, .. } => spring_state(*spec, *x0, *v0, elapsed_since(*until, now)).1,
        }
    }

    fn settled_at(&self, now: Instant) -> bool {
        match self {
            Self::Rest(_) => true,
            Self::Eased { start, dur, .. } => now >= *start + *dur,
            Self::Spring { start, x0, v0, spec, .. } => spring_near_rest(*spec, *x0, *v0, *start, now),
            Self::Trail { until, .. } if now < *until => false,
            Self::Trail { until, x0, v0, spec, .. } => spring_near_rest(*spec, *x0, *v0, *until, now),
        }
    }

    /// Collapses a trail whose delay has elapsed into the spring it started, so carriers stay shallow.
    fn advance(&mut self, now: Instant) {
        let elapsed = match self {
            Self::Trail { until, .. } => *until <= now,
            _ => false,
        };
        if !elapsed {
            return;
        }
        if let Self::Trail { until, target, x0, v0, spec, .. } = &*self {
            *self = Self::Spring { start: *until, target: *target, x0: *x0, v0: *v0, spec: *spec };
        }
    }
}

/// Closed-form underdamped spring about a target: returns the offset from the target and the velocity at `t` seconds.
fn spring_state(spec: SpringSpec, x0: f32, v0: f32, t: f32) -> (f32, f32) {
    let decay = spec.zeta * spec.omega0;
    let omega_d = spec.omega0 * (1. - spec.zeta * spec.zeta).sqrt();
    let (sin, cos) = (omega_d * t).sin_cos();
    let envelope = (-decay * t).exp();
    let scaled = (v0 + decay * x0) / omega_d;
    let x = envelope * (x0 * cos + scaled * sin);
    let v = envelope * (v0 * cos - (decay * scaled + x0 * omega_d) * sin);
    (x, v)
}

fn spring_near_rest(spec: SpringSpec, x0: f32, v0: f32, start: Instant, now: Instant) -> bool {
    let (x, v) = spring_state(spec, x0, v0, elapsed_since(start, now));
    x.abs() <= 0.25 && v.abs() < 2.
}

/// Seconds from `start` to `now`, never negative: sampling before a motion starts draws its first frame.
fn elapsed_since(start: Instant, now: Instant) -> f32 {
    now.checked_duration_since(start).map_or(0., |d| d.as_secs_f32())
}

fn eased_value(from: f32, target: f32, curve: Ease, start: Instant, dur: Duration, now: Instant) -> f32 {
    let secs = dur.as_secs_f32();
    if secs <= 0. {
        return target;
    }
    let progress = (elapsed_since(start, now) / secs).clamp(0., 1.);
    from + (target - from) * curve.apply(progress)
}

fn eased_velocity(from: f32, target: f32, curve: Ease, start: Instant, dur: Duration, now: Instant) -> f32 {
    let secs = dur.as_secs_f32();
    if secs <= 0. {
        return 0.;
    }
    let progress = (elapsed_since(start, now) / secs).clamp(0., 1.);
    if progress >= 1. {
        return 0.;
    }
    (target - from) * curve.slope(progress) / secs
}

/// Clamped so no finite timing can make `from_secs_f32` panic; non-positive durations mean "immediate".
fn duration_from_ms(ms: f32) -> Duration {
    Duration::from_secs_f32((ms / 1000.).clamp(0., 60.))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn retarget_and_trailing_delay_stay_continuous() {
        let t0 = Instant::now();
        let mut morph = Morph::new(182., 44.);
        morph.retarget(188., 50., MorphKind::Capsule, Motion::Full, t0);

        let t1 = t0 + 100 * MS;
        let before = morph.sample(t1 - MS);
        morph.retarget(177., 44., MorphKind::Capsule, Motion::Full, t1);
        let after = morph.sample(t1);
        assert!((before.0 - after.0).abs() < 0.5);
        assert!((before.1 - after.1).abs() < 0.5);

        let until = t1 + 64 * MS;
        let pre_trail = morph.sample(until - MS);
        let post_trail = morph.sample(until);
        assert!((pre_trail.1 - post_trail.1).abs() < 0.5);
        // The trailing axis keeps its old motion during the delay instead of freezing.
        assert!((morph.sample(t1 + 30 * MS).1 - after.1).abs() > 0.01);
    }

    #[test]
    fn collapse_dips_inward_and_expand_overshoots() {
        let t0 = Instant::now();
        let mut collapse = Morph::new(776., 197.);
        collapse.retarget(89., 50., MorphKind::Collapse, Motion::Full, t0);
        let mut narrowest = f32::MAX;
        for step in 0..=120 {
            let (width, _) = collapse.sample(t0 + step * 10 * MS);
            narrowest = narrowest.min(width);
        }
        assert!(narrowest < 89.);
        assert!(collapse.settled(t0 + 1200 * MS));
        let (width, height) = collapse.sample(t0 + 1200 * MS);
        assert!((width - 89.).abs() <= 0.25 && (height - 50.).abs() <= 0.25);

        let mut expand = Morph::new(188., 50.);
        expand.retarget(776., 197., MorphKind::Expand, Motion::Full, t0);
        let mut widest = f32::MIN;
        for step in 0..=120 {
            let (width, _) = expand.sample(t0 + step * 10 * MS);
            widest = widest.max(width);
        }
        assert!(widest > 776.);
    }

    #[test]
    fn fade_retarget_starts_from_the_drawn_value() {
        let t0 = Instant::now();
        let mut fade = Fade::new(0.);
        fade.retarget(1., Timing::materialize_out(Motion::Full), t0);
        let halfway = t0 + 60 * MS;
        let drawn = fade.sample(halfway);
        assert!(drawn > 0. && drawn < 1.);
        fade.retarget(0., Timing::materialize_in(Motion::Full, 0.), halfway);
        assert!((fade.sample(halfway) - drawn).abs() < 0.01);
        assert!(fade.sample(halfway + 180 * MS).abs() < 0.001);
    }
}
