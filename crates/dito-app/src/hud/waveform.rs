//! Presentation-only RMS normalization and inertial bars. Never changes recorded audio.
//! RMS is an amplitude envelope, not a frequency spectrum or a speech classifier.

pub(crate) const REST: [f32; 8] = [6., 11., 18., 10., 15., 8., 13., 9.];
const PROFILE: [f32; 8] = [0.58, 0.78, 0.96, 0.84, 1., 0.73, 0.88, 0.62];

pub(crate) struct Waveform {
    floor: f32,
    peak: f32,
    primed: bool,
    taps: [f32; 8],
    heights: [f32; 8],
    velocities: [f32; 8],
}

impl Default for Waveform {
    fn default() -> Self {
        Self { floor: 0.00006, peak: 0.0002, primed: false, taps: [0.; 8], heights: [3.; 8], velocities: [0.; 8] }
    }
}

impl Waveform {
    /// Time-based coefficients keep the same response at different refresh rates.
    pub(crate) fn advance(&mut self, rms: f32, elapsed: f32) {
        let rms = if rms.is_finite() { rms.clamp(0., 1.) } else { 0. };
        // Substeps preserve the propagation delay during a slow frame without replaying a long stall.
        let mut remaining = elapsed.clamp(0., 0.25);
        while remaining > 0.00001 {
            let dt = remaining.min(1. / 120.);
            remaining -= dt;
            self.step(rms, dt);
        }
    }

    fn step(&mut self, rms: f32, dt: f32) {
        // Lower-envelope estimate: follow quieter gaps quickly, rise very slowly so a sustained
        // word is not learned as silence. The absolute floor avoids amplifying numerical silence.
        let floor_target = rms.max(0.00003);
        let floor_tau = if floor_target < self.floor { 0.25 } else { 30. };
        self.floor += (floor_target - self.floor) * response(dt, floor_tau);
        let threshold = (self.floor * 1.4).max(0.00008);
        let signal = (rms - threshold).max(0.);
        let gate = (signal / (self.floor * 2. + 0.00006)).clamp(0., 1.);
        if gate > 0.1 && !self.primed {
            self.peak = rms.max(0.0002);
            self.primed = true;
        }
        let peak_tau = if rms > self.peak { 0.045 } else { 3. };
        self.peak += (rms - self.peak) * response(dt, peak_tau);
        let reference = (self.peak - threshold).max(0.0001);
        // Soft compression leaves headroom for accents instead of pinning loud input to a ceiling.
        let relative = 1. - (-2. * signal / reference).exp();
        let amplitude = gate * (0.18 + 0.82 * relative);
        // Neighboring bars follow recent input with a small delay; no free-running sine in silence.
        let before = self.taps;
        for i in 0..8 {
            let target = if i == 0 { amplitude } else { before[i - 1] };
            self.taps[i] += (target - self.taps[i]) * response(dt, 0.025);
            let target_height = 3. + 15. * PROFILE[i] * self.taps[i];
            // Exact critically damped spring: a fast attack and a gentler release, no oscillation.
            let omega = if target_height > self.heights[i] { 38. } else { 18. };
            let delta = self.heights[i] - target_height;
            let velocity = self.velocities[i];
            let c = velocity + omega * delta;
            let decay = (-omega * dt).exp();
            self.heights[i] = (target_height + (delta + c * dt) * decay).clamp(3., 18.);
            self.velocities[i] = (velocity - omega * c * dt) * decay;
        }
    }

    pub(crate) fn heights(&self) -> [f32; 8] {
        self.heights
    }

    /// The onboarding scene uses the same filter with a repeatable speaking envelope.
    pub(crate) fn demo(t: f32) -> [f32; 8] {
        let end = t.max(0.) % 4.;
        let mut model = Self::default();
        let mut time = 0.;
        while time < end {
            let dt = (end - time).min(1. / 60.);
            model.advance(speech_sample(time, 0.025), dt);
            time += dt;
        }
        model.heights()
    }
}

fn response(dt: f32, tau: f32) -> f32 {
    1. - (-dt / tau).exp()
}

/// Controlled input for the onboarding demonstration and native visual checks; never the live mic.
pub(crate) fn speech_sample(t: f32, gain: f32) -> f32 {
    let t = t % 4.;
    if !(0.35..2.8).contains(&t) {
        return 0.;
    }
    let syllable = (t * 13.).sin().max(0.).powi(2);
    let phrase = 0.5 + 0.5 * (t * 3.1).sin();
    gain * (0.08 + 0.92 * syllable) * (0.35 + 0.65 * phrase)
}

#[cfg(test)]
mod tests {
    use super::{Waveform, speech_sample};
    fn phrase(gain: f32, hz: f32) -> Vec<[f32; 8]> {
        let mut model = Waveform::default();
        (0..(4. * hz) as usize)
            .map(|frame| {
                model.advance(speech_sample(frame as f32 / hz, gain), 1. / hz);
                model.heights()
            })
            .collect()
    }

    #[test]
    fn whispers_and_loud_speech_both_move_without_flattening() {
        for gain in [0.0006, 0.006, 0.06, 0.6] {
            let frames = phrase(gain, 60.);
            let moving: Vec<_> = frames[30..150].iter().map(|h| h[4]).collect();
            let (lo, hi) = moving.iter().fold((15.0_f32, 0.0_f32), |(lo, hi), &v| (lo.min(v), hi.max(v)));
            assert!(hi > 7., "quiet input must be visible: {gain}, {hi}");
            assert!(hi - lo > 2., "syllables retain contrast: {gain}, {lo}..{hi}");
            assert!(frames.iter().flatten().all(|v| v.is_finite() && (3. ..=18.).contains(v)));
        }
    }

    #[test]
    fn silence_is_still_and_release_is_smooth() {
        let mut model = Waveform::default();
        for _ in 0..600 {
            model.advance(0.00002, 1. / 60.);
        }
        assert_eq!(model.heights(), [3.; 8]);
        for _ in 0..60 {
            model.advance(0.02, 1. / 60.);
        }
        let before = model.heights()[4];
        model.advance(0., 1. / 60.);
        assert!(model.heights()[4] > before * 0.9, "no snap to silence");
        for _ in 0..90 {
            model.advance(0., 1. / 60.);
        }
        assert!(model.heights().iter().all(|&h| h < 3.01), "tail settles in silence");
    }

    #[test]
    fn response_is_consistent_across_refresh_rates_and_invalid_input() {
        let slow = phrase(0.006, 30.);
        let fast = phrase(0.006, 120.);
        let average_error = slow
            .iter()
            .zip(fast.as_chunks::<4>().0.iter())
            .map(|(a, b)| a.iter().zip(b[3]).map(|(x, y)| (x - y).abs()).sum::<f32>() / 8.)
            .sum::<f32>()
            / slow.len() as f32;
        assert!(average_error < 0.85, "refresh-rate drift: {average_error}");
        let mut model = Waveform::default();
        for value in [f32::NAN, f32::INFINITY, -1., 10.] {
            model.advance(value, 1. / 60.);
        }
        assert!(model.heights().iter().all(|v| v.is_finite() && (3. ..=18.).contains(v)));
    }
}
