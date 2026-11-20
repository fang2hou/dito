use std::sync::OnceLock;

static REDUCE_MOTION_SOURCE: OnceLock<fn() -> bool> = OnceLock::new();

pub fn install_reduce_motion_source(source: fn() -> bool) {
    let _ = REDUCE_MOTION_SOURCE.set(source);
}

pub(super) fn reduce_motion() -> bool {
    REDUCE_MOTION_SOURCE.get().is_some_and(|source| source())
}

/// Follows the system's Reduce Motion setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Full,
    Reduced,
}

impl Motion {
    pub fn system() -> Self {
        if reduce_motion() { Self::Reduced } else { Self::Full }
    }

    pub fn is_reduced(self) -> bool {
        self == Self::Reduced
    }
}

/// Smoothstep without a clamp: callers whose `t` can leave 0 to 1 clamp it first.
pub fn ease(t: f32) -> f32 {
    t * t * (3. - 2. * t)
}

pub fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

/// An underdamped spring, by its decay rate and damped frequency.
#[derive(Clone, Copy)]
pub struct SpringCurve {
    pub decay: f32,
    pub frequency: f32,
}

impl SpringCurve {
    /// The app-wide glass curve, with time measured in its 460 ms window.
    pub const GLASS: Self = Self { decay: 5.5, frequency: 5.3 };

    pub fn damped(damping: f32, omega: f32) -> Self {
        Self { decay: damping * omega, frequency: omega * (1. - damping * damping).sqrt() }
    }

    /// The closed form about the target: starting `offset` away with `velocity`, the offset and velocity `time` later.
    pub fn step(self, offset: f32, velocity: f32, time: f32) -> (f32, f32) {
        let Self { decay, frequency } = self;
        let b = (velocity + decay * offset) / frequency;
        let (sin, cos) = (frequency * time).sin_cos();
        let e = (-decay * time).exp();
        let wave = offset * cos + b * sin;
        (e * wave, e * (frequency * (b * cos - offset * sin) - decay * wave))
    }
}
