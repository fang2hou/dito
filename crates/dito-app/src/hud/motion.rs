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
