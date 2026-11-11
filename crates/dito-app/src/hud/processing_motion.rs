//! The displayed fill owns its clock. A result retargets the current position and velocity.

#[derive(Clone, Copy, Default)]
pub(super) struct Fill {
    pub value: f32,
    speed: f32,
}

impl Fill {
    pub fn at_rest(value: f32) -> Self {
        Self { value, speed: 0. }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ProcessingMotion {
    from: Fill,
    target: f32,
    pub catch_up: f32,
    estimate: Option<f32>,
}

impl ProcessingMotion {
    pub fn transcribing(estimate: f32, reduced: bool) -> Self {
        Self { from: Fill::default(), target: 0., catch_up: 0., estimate: (!reduced).then_some(estimate) }
    }

    pub fn polishing(from: Fill, estimate: f32, already_running: f32, reduced: bool) -> Self {
        let mut motion = Self::retarget(from, 0.5, reduced);
        // Account for time spent in the backend during the bridge without jumping ahead of the displayed midpoint.
        motion.estimate = (!reduced).then_some((estimate - already_running - motion.catch_up).max(0.25));
        motion
    }

    pub fn finished(from: Fill, reduced: bool) -> Self {
        Self::retarget(from, 1., reduced)
    }

    fn retarget(mut from: Fill, target: f32, reduced: bool) -> Self {
        let gap = (target - from.value).max(0.);
        if gap == 0. {
            return Self { from: Fill::at_rest(target), target, catch_up: 0., estimate: None };
        }
        let catch_up = if reduced { 0.12 } else { 0.10 + 0.10 * (gap / 0.5).min(1.) };
        // A monotone Hermite segment carries the incoming speed, within the bound that prevents overshoot.
        from.speed = from.speed.clamp(0., 3. * gap / catch_up);
        Self { from, target, catch_up, estimate: None }
    }

    pub fn at(self, elapsed: f32) -> Fill {
        let t = elapsed.max(0.);
        if t < self.catch_up {
            let u = t / self.catch_up;
            let gap = self.target - self.from.value;
            let value =
                self.from.value + gap * u * u * (3. - 2. * u) + self.from.speed * self.catch_up * u * (1. - u).powi(2);
            let speed = gap * 6. * u * (1. - u) / self.catch_up + self.from.speed * (1. - u) * (1. - 3. * u);
            return Fill { value: value.clamp(self.from.value, self.target), speed: speed.max(0.) };
        }
        let Some(estimate) = self.estimate else { return Fill { value: self.target, speed: 0. } };
        let t = t - self.catch_up;
        let ramp = 0.18;
        let startup = (-t / ramp).exp();
        let effective_time = (t - ramp * (1. - startup)).max(0.);
        let rate = 2.3 / estimate.max(0.25);
        let remaining = (-rate * effective_time).exp();
        let span = 0.485;
        Fill { value: self.target + span * (1. - remaining), speed: span * remaining * rate * (1. - startup) }
    }
}

#[cfg(test)]
mod tests {
    use super::{Fill, ProcessingMotion};

    #[test]
    fn starts_at_zero_and_never_claims_a_result_while_waiting() {
        let asr = ProcessingMotion::transcribing(2., false);
        assert_eq!(asr.at(0.).value, 0.);
        assert!(asr.at(0.05).value < 0.01);
        assert!(asr.at(3.).value > asr.at(2.).value);
        assert!(asr.at(60.).value < 0.5);
        let polish = ProcessingMotion::polishing(asr.at(0.05), 4., 0., false);
        assert_eq!(polish.at(polish.catch_up).value, 0.5);
        assert!(polish.at(60.).value < 1.);
    }

    #[test]
    fn a_fast_second_result_retargets_without_a_jump_or_a_second_wait() {
        let polish = ProcessingMotion::polishing(Fill::default(), 2., 0., false);
        let in_flight = polish.at(0.06);
        let finished = ProcessingMotion::finished(in_flight, false);
        assert_eq!(finished.at(0.).value, in_flight.value);
        assert!((finished.at(0.).speed - in_flight.speed).abs() < 1e-5);
        let mut previous = in_flight.value;
        for ms in 1..=200 {
            let next = finished.at(ms as f32 / 1000.).value;
            assert!(next >= previous && next <= 1.);
            previous = next;
        }
        assert_eq!(previous, 1.);
        let coalesced = ProcessingMotion::finished(Fill::default(), false);
        assert_eq!(coalesced.at(0.).value, 0.);
        assert_eq!(coalesced.at(0.2).value, 1.);
    }

    #[test]
    fn reduced_motion_updates_only_after_a_result() {
        let asr = ProcessingMotion::transcribing(1., true);
        assert_eq!(asr.at(10.).value, 0.);
        let polish = ProcessingMotion::polishing(asr.at(10.), 2., 0., true);
        assert_eq!(polish.at(0.).value, 0.);
        assert_eq!(polish.at(0.12).value, 0.5);
        assert_eq!(polish.at(10.).value, 0.5);
    }
}
