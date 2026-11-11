//! Streaming windowed-sinc resampler to 16 kHz.

const HALF_TAPS: usize = 24;

pub struct Resampler {
    /// Input samples consumed per output sample.
    step: f64,
    /// Low-pass cutoff relative to the input Nyquist (1.0 = no filtering).
    cutoff: f64,
    buf: Vec<f32>,
    /// Position of the next output sample, in `buf` indices.
    pos: f64,
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-9 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) }
}

impl Resampler {
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        let step = in_rate as f64 / out_rate as f64;
        Self {
            step,
            cutoff: (1.0 / step).min(1.0),
            // History of zeros so the first outputs have a full kernel.
            buf: vec![0.0; HALF_TAPS],
            pos: HALF_TAPS as f64,
        }
    }

    /// Feed input samples; resampled output is appended to `out`.
    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.buf.extend_from_slice(input);
        while self.pos as usize + HALF_TAPS < self.buf.len() {
            let base = self.pos.floor();
            let frac = self.pos - base;
            let base = base as usize;
            let mut acc = 0.0f64;
            for k in 0..(2 * HALF_TAPS) {
                let idx = base + k + 1 - HALF_TAPS;
                let x = (k as f64 + 1.0 - HALF_TAPS as f64) - frac;
                let win = 0.5 * (1.0 + (std::f64::consts::PI * x / HALF_TAPS as f64).cos());
                acc += self.buf[idx] as f64 * self.cutoff * sinc(self.cutoff * x) * win;
            }
            out.push(acc as f32);
            self.pos += self.step;
        }
        // Drop input that no future output can reference.
        let keep_from = (self.pos as usize).saturating_sub(HALF_TAPS);
        if keep_from > 0 {
            self.buf.drain(..keep_from);
            self.pos -= keep_from as f64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, hz: f32, secs: f32) -> Vec<f32> {
        (0..(rate as f32 * secs) as usize)
            .map(|i| (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin() * 0.5)
            .collect()
    }

    fn zero_crossings(x: &[f32]) -> usize {
        x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
    }

    #[test]
    fn output_length_follows_the_rate_ratio() {
        for rate in [48_000u32, 44_100, 16_000, 24_000] {
            let mut out = Vec::new();
            Resampler::new(rate, 16_000).push(&tone(rate, 440.0, 1.0), &mut out);
            assert!((out.len() as i64 - 16_000).abs() < 60, "{rate}: got {}", out.len());
        }
    }

    #[test]
    fn a_tone_keeps_its_pitch_and_level() {
        let mut out = Vec::new();
        Resampler::new(48_000, 16_000).push(&tone(48_000, 1000.0, 1.0), &mut out);
        let body = &out[200..out.len() - 200];
        assert!((zero_crossings(body) as i64 - 1000 * body.len() as i64 / 16_000).abs() <= 2);
        let peak = body.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((peak - 0.5).abs() < 0.03, "peak {peak}");
    }

    #[test]
    fn content_above_the_new_nyquist_is_suppressed() {
        let mut out = Vec::new();
        Resampler::new(48_000, 16_000).push(&tone(48_000, 12_000.0, 1.0), &mut out);
        let peak = out[200..out.len() - 200].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak < 0.05, "aliasing leaked: {peak}");
    }

    #[test]
    fn chunked_input_matches_one_shot_input() {
        let src = tone(44_100, 300.0, 0.5);
        let mut whole = Vec::new();
        Resampler::new(44_100, 16_000).push(&src, &mut whole);
        let mut parts = Vec::new();
        let mut r = Resampler::new(44_100, 16_000);
        for c in src.chunks(441) {
            r.push(c, &mut parts);
        }
        assert_eq!(whole.len(), parts.len());
        assert!(whole.iter().zip(&parts).all(|(a, b)| (a - b).abs() < 1e-5));
    }
}
