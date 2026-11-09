//! Analytic damped springs: no frame-rate-dependent integration or velocity reset on retarget.

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Spring {
    pub position: f32,
    pub velocity: f32,
}

impl Spring {
    pub fn advance(&mut self, target: f32, seconds: f32, damping: f32, omega: f32) {
        if seconds <= 0. {
            return;
        }
        let decay = damping * omega;
        let wd = omega * (1. - damping * damping).sqrt();
        let displacement = self.position - target;
        let b = (self.velocity + decay * displacement) / wd;
        let (sin, cos) = (wd * seconds).sin_cos();
        let e = (-decay * seconds).exp();
        let wave = displacement * cos + b * sin;
        self.position = target + e * wave;
        self.velocity = e * (wd * (b * cos - displacement * sin) - decay * wave);
    }

    pub fn moving(&self, target: f32) -> bool {
        (self.position - target).abs() > 0.001 || self.velocity.abs() > 0.01
    }
}

pub(super) fn spring_progress(seconds: f32, damping: f32, omega: f32) -> f32 {
    let mut spring = Spring::default();
    spring.advance(1., seconds.max(0.), damping, omega);
    spring.position
}

pub(super) fn smooth(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// Monotone Hermite segments join the approved poses without adding another rebound.
pub(super) fn curve(time: f32, poses: &[(f32, f32)]) -> f32 {
    let slope = |i: usize| {
        if i == 0 || i + 1 == poses.len() {
            return 0.;
        }
        let before = (poses[i].1 - poses[i - 1].1) / (poses[i].0 - poses[i - 1].0);
        let after = (poses[i + 1].1 - poses[i].1) / (poses[i + 1].0 - poses[i].0);
        if before * after <= 0. { 0. } else { 2. * before * after / (before + after) }
    };
    for (i, pair) in poses.windows(2).enumerate() {
        if time <= pair[1].0 {
            let dt = pair[1].0 - pair[0].0;
            let u = ((time - pair[0].0) / dt).clamp(0., 1.);
            return (2. * u.powi(3) - 3. * u * u + 1.) * pair[0].1
                + (u.powi(3) - 2. * u * u + u) * dt * slope(i)
                + (-2. * u.powi(3) + 3. * u * u) * pair[1].1
                + (u.powi(3) - u * u) * dt * slope(i + 1);
        }
    }
    poses.last().map_or(0., |pose| pose.1)
}

#[derive(Clone, Copy)]
pub(crate) struct EntryPose {
    pub width: f32,
    pub height: f32,
    pub y: f32,
    pub button: f32,
    pub alpha: f32,
}

impl Default for EntryPose {
    fn default() -> Self {
        Self { width: 1., height: 1., y: 0., button: 1., alpha: 1. }
    }
}

impl EntryPose {
    pub fn at(seconds: f32, motion: crate::ui::Motion) -> Self {
        if motion.is_reduced() {
            return Self { alpha: smooth(seconds / 0.1), ..Self::default() };
        }
        Self {
            width: curve(seconds, &[(0., 0.48), (0.08, 0.76), (0.15, 1.03), (0.30, 1.)]),
            height: curve(seconds, &[(0., 0.52), (0.08, 1.16), (0.15, 1.05), (0.30, 1.)]),
            y: curve(seconds, &[(0., 12.), (0.08, -3.), (0.15, -1.), (0.30, 0.)]),
            button: curve(seconds, &[(0., 0.20), (0.08, 0.72), (0.15, 1.04), (0.30, 1.)]),
            alpha: smooth(seconds / 0.08),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_is_frame_rate_independent_and_retargets_without_losing_velocity() {
        let mut whole = Spring::default();
        whole.advance(1., 0.16, 0.72, 22.);
        let mut frames = Spring::default();
        for _ in 0..16 {
            frames.advance(1., 0.01, 0.72, 22.);
        }
        assert!((whole.position - frames.position).abs() < 0.00001);
        assert!((whole.velocity - frames.velocity).abs() < 0.0001);
        let before = frames;
        frames.advance(0., 0.00001, 0.72, 22.);
        assert!((frames.position - before.position).abs() < 0.001);
        assert!((frames.velocity - before.velocity).abs() < 0.01);
        frames.advance(0., 1., 0.72, 22.);
        assert!(!frames.moving(0.));
    }
}
