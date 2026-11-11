use std::time::Instant;

const SPRING_OMEGA: f64 = 18.;
const SPRING_ZETA: f64 = 0.9;
const REDUCED_MOTION_RATE: f64 = 24.;
/// A frame gap longer than this (sleep, a stall) snaps to the target instead of integrating the spring.
const MAX_STEP_SECS: f64 = 0.25;
const SETTLE_DISTANCE: f64 = 0.02;
const SETTLE_SPEED: f64 = 0.1;

/// What the HUD draws spans this far above the window's bottom edge. Transparent window space outside it
/// may leave the screen.
pub(super) const FOOTPRINT_BOTTOM: f64 = 15.;
const FOOTPRINT_TOP: f64 = 112.;
const CAPSULE_CENTER_Y: f64 = 61.5;
const CAPSULE_HALF_WIDTH: f64 = 52.5;
const CAPSULE_HALF_HEIGHT: f64 = 16.5;
const CURSOR_GAP: f64 = 5.;
const FLIPPED_DROP: f64 = 132.;

const FREEZE_ENTER_DISTANCE: f64 = 64.;
const FREEZE_EXIT_DISTANCE: f64 = 80.;
const FREEZE_APPROACH_SPEED: f64 = 0.1;

type Point = (f64, f64);

/// The window holds still while the pointer works the HUD's controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerContact {
    Away,
    OnControls,
}

/// In AppKit's global coordinates.
#[derive(Debug, Clone, Copy)]
pub(super) struct Area {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Bounds {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
}

#[derive(Default)]
pub struct HudFollowState {
    pub(super) position: Option<Point>,
    velocity: Point,
    cursor: Option<Point>,
    last_tick: Option<Instant>,
    frozen: bool,
}

pub(super) struct FollowFrame {
    pub cursor: Point,
    pub window_width: f64,
    pub target: Point,
    pub bounds: Bounds,
    pub contact: PointerContact,
    pub reduced_motion: bool,
}

impl HudFollowState {
    /// `current` is the window origin as last placed. Returns `None` while the window holds still for the pointer.
    pub(super) fn step(&mut self, current: Point, frame: &FollowFrame, now: Instant) -> Option<Point> {
        let capsule = (current.0 + frame.window_width / 2., current.1 + CAPSULE_CENTER_Y);
        self.freeze_for_pointer(frame, capsule);
        self.cursor = Some(frame.cursor);
        let dt = self.last_tick.replace(now).map_or(0., |last| now.duration_since(last).as_secs_f64());
        if self.frozen {
            self.velocity = (0., 0.);
            self.position = Some(current);
            return None;
        }
        let position = *self.position.get_or_insert(frame.target);
        let reduced = frame.reduced_motion;
        let (x, vx) = settle(follow_axis(position.0, self.velocity.0, frame.target.0, dt, reduced), frame.target.0);
        let (y, vy) = settle(follow_axis(position.1, self.velocity.1, frame.target.1, dt, reduced), frame.target.1);
        let next = (x.clamp(frame.bounds.x_min, frame.bounds.x_max), y.clamp(frame.bounds.y_min, frame.bounds.y_max));
        self.position = Some(next);
        self.velocity = (if x == next.0 { vx } else { 0. }, if y == next.1 { vy } else { 0. });
        Some(next)
    }

    fn freeze_for_pointer(&mut self, frame: &FollowFrame, capsule: Point) {
        let dx = ((frame.cursor.0 - capsule.0).abs() - CAPSULE_HALF_WIDTH).max(0.);
        let dy = ((frame.cursor.1 - capsule.1).abs() - CAPSULE_HALF_HEIGHT).max(0.);
        let distance = dx.hypot(dy);
        let approach = self.cursor.map_or(0., |old| {
            (frame.cursor.0 - old.0) * (capsule.0 - frame.cursor.0)
                + (frame.cursor.1 - old.1) * (capsule.1 - frame.cursor.1)
        });
        self.frozen = freeze_for_interaction(self.frozen, frame.contact, distance, approach);
    }
}

fn settle((position, velocity): Point, target: f64) -> Point {
    if (position - target).abs() < SETTLE_DISTANCE && velocity.abs() < SETTLE_SPEED {
        (target, 0.)
    } else {
        (position, velocity)
    }
}

/// Exact underdamped spring step with a target held constant during this frame.
fn follow_axis(position: f64, velocity: f64, target: f64, dt: f64, reduced: bool) -> Point {
    if dt <= 0. {
        return (position, velocity);
    }
    if dt > MAX_STEP_SECS {
        return (target, 0.);
    }
    if reduced {
        let remaining = (-REDUCED_MOTION_RATE * dt).exp();
        return (target + (position - target) * remaining, 0.);
    }
    let decay = SPRING_ZETA * SPRING_OMEGA;
    let wd = SPRING_OMEGA * (1. - SPRING_ZETA * SPRING_ZETA).sqrt();
    let d = position - target;
    let b = (velocity + decay * d) / wd;
    let (sin, cos) = (wd * dt).sin_cos();
    let e = (-decay * dt).exp();
    let wave = d * cos + b * sin;
    (target + e * wave, e * (wd * (b * cos - d * sin) - decay * wave))
}

fn freeze_for_interaction(frozen: bool, contact: PointerContact, distance: f64, approach: f64) -> bool {
    if contact == PointerContact::OnControls
        || (!frozen && distance < FREEZE_ENTER_DISTANCE && approach > FREEZE_APPROACH_SPEED)
    {
        true
    } else {
        frozen && distance <= FREEZE_EXIT_DISTANCE
    }
}

pub(super) fn pointer_target(cursor: Point, window_width: f64, area: Area) -> (Point, Bounds) {
    let bounds = Bounds {
        x_min: area.x,
        x_max: (area.x + area.width - window_width).max(area.x),
        y_min: area.y - FOOTPRINT_BOTTOM,
        y_max: (area.y + area.height - FOOTPRINT_TOP).max(area.y - FOOTPRINT_BOTTOM),
    };
    let x = (cursor.0 - window_width / 2.).clamp(bounds.x_min, bounds.x_max);
    let above = cursor.1 + CURSOR_GAP;
    let y = (if above <= bounds.y_max { above } else { cursor.1 - FLIPPED_DROP }).clamp(bounds.y_min, bounds.y_max);
    ((x, y), bounds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_spring_is_frame_rate_independent_and_handles_sleep() {
        let once = follow_axis(0., 0., 100., 0.1, false);
        let first = follow_axis(0., 0., 100., 0.04, false);
        let split = follow_axis(first.0, first.1, 100., 0.06, false);
        assert!((once.0 - split.0).abs() < 0.0001);
        assert!((once.1 - split.1).abs() < 0.0001);
        assert!(once.0 > 0. && once.0 < 100.);
        assert_eq!(follow_axis(0., 0., 100., 1., false), (100., 0.));
        let reduced = follow_axis(0., 0., 100., 0.1, true);
        assert!(reduced.0 > 0. && reduced.0 < 100.);
    }
}
