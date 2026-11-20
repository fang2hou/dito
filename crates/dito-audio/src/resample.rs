//! Streaming windowed-sinc resampler to 16 kHz.

const HALF_TAPS: usize = 24;
const TAP_COUNT: usize = 2 * HALF_TAPS;
const MAX_PHASES: usize = 1024;

enum Filter {
    Passthrough,
    Polyphase { taps: Vec<f64>, phase: usize, phase_count: usize, phase_step: usize, whole_step: usize, pos: usize },
    Direct { step: f64, cutoff: f64, pos: f64 },
}

pub struct Resampler {
    buf: Vec<f32>,
    filter: Filter,
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-9 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) }
}

fn greatest_common_divisor(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn tap(k: usize, frac: f64, cutoff: f64) -> f64 {
    let x = (k as f64 + 1.0 - HALF_TAPS as f64) - frac;
    let window = 0.5 * (1.0 + (std::f64::consts::PI * x / HALF_TAPS as f64).cos());
    cutoff * sinc(cutoff * x) * window
}

impl Resampler {
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        if in_rate == out_rate {
            return Self { buf: Vec::new(), filter: Filter::Passthrough };
        }

        let step = in_rate as f64 / out_rate as f64;
        let cutoff = (1.0 / step).min(1.0);
        let divisor = greatest_common_divisor(in_rate, out_rate);
        let phase_count = (out_rate / divisor) as usize;
        let filter = if phase_count <= MAX_PHASES {
            let phase_step_numerator = (in_rate / divisor) as usize;
            let taps = (0..phase_count)
                .flat_map(|phase| {
                    let frac = phase as f64 / phase_count as f64;
                    (0..TAP_COUNT).map(move |k| tap(k, frac, cutoff))
                })
                .collect();
            Filter::Polyphase {
                taps,
                phase: 0,
                phase_count,
                phase_step: phase_step_numerator % phase_count,
                whole_step: phase_step_numerator / phase_count,
                pos: HALF_TAPS,
            }
        } else {
            Filter::Direct { step, cutoff, pos: HALF_TAPS as f64 }
        };

        Self {
            // History of zeros so the first outputs have a full kernel.
            buf: vec![0.0; HALF_TAPS],
            filter,
        }
    }

    /// Feed input samples; resampled output is appended to `out`.
    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if matches!(self.filter, Filter::Passthrough) {
            out.extend_from_slice(input);
            return;
        }

        self.buf.extend_from_slice(input);
        let keep_from = match &mut self.filter {
            Filter::Polyphase { taps, phase, phase_count, phase_step, whole_step, pos } => {
                while *pos + HALF_TAPS < self.buf.len() {
                    let tap_offset = *phase * TAP_COUNT;
                    let mut acc = 0.0f64;
                    for k in 0..TAP_COUNT {
                        acc += self.buf[*pos + k + 1 - HALF_TAPS] as f64 * taps[tap_offset + k];
                    }
                    out.push(acc as f32);

                    *pos += *whole_step;
                    *phase += *phase_step;
                    if *phase >= *phase_count {
                        *phase -= *phase_count;
                        *pos += 1;
                    }
                }
                pos.saturating_sub(HALF_TAPS)
            }
            Filter::Direct { step, cutoff, pos } => {
                while *pos as usize + HALF_TAPS < self.buf.len() {
                    let base = pos.floor();
                    let frac = *pos - base;
                    let base = base as usize;
                    let mut acc = 0.0f64;
                    for k in 0..TAP_COUNT {
                        let idx = base + k + 1 - HALF_TAPS;
                        acc += self.buf[idx] as f64 * tap(k, frac, *cutoff);
                    }
                    out.push(acc as f32);
                    *pos += *step;
                }
                (*pos as usize).saturating_sub(HALF_TAPS)
            }
            Filter::Passthrough => unreachable!(),
        };

        if keep_from > 0 {
            self.buf.drain(..keep_from);
            match &mut self.filter {
                Filter::Polyphase { pos, .. } => *pos -= keep_from,
                Filter::Direct { pos, .. } => *pos -= keep_from as f64,
                Filter::Passthrough => unreachable!(),
            }
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

    struct DirectReference {
        step: f64,
        cutoff: f64,
        buf: Vec<f32>,
        pos: f64,
    }

    impl DirectReference {
        fn new(in_rate: u32, out_rate: u32) -> Self {
            let step = in_rate as f64 / out_rate as f64;
            Self { step, cutoff: (1.0 / step).min(1.0), buf: vec![0.0; HALF_TAPS], pos: HALF_TAPS as f64 }
        }

        fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
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
            let keep_from = (self.pos as usize).saturating_sub(HALF_TAPS);
            if keep_from > 0 {
                self.buf.drain(..keep_from);
                self.pos -= keep_from as f64;
            }
        }
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

    #[test]
    fn polyphase_output_matches_direct_reference() {
        let mut largest_difference = 0.0f32;
        let chunk_sizes = [1, 17, 3, 509, 64, 1021, 7, 241];

        for rate in [48_000, 44_100] {
            let src = tone(rate, 1_337.0, 0.25);
            let mut actual = Resampler::new(rate, 16_000);
            let mut reference = DirectReference::new(rate, 16_000);
            let mut actual_output = Vec::new();
            let mut reference_output = Vec::new();
            let mut offset = 0;
            let mut chunk_index = 0;
            while offset < src.len() {
                let end = (offset + chunk_sizes[chunk_index % chunk_sizes.len()]).min(src.len());
                actual.push(&src[offset..end], &mut actual_output);
                reference.push(&src[offset..end], &mut reference_output);
                offset = end;
                chunk_index += 1;
            }

            assert_eq!(actual_output.len(), reference_output.len(), "{rate}");
            for (actual, reference) in actual_output.iter().zip(&reference_output) {
                largest_difference = largest_difference.max((actual - reference).abs());
            }
        }

        println!("largest reference difference: {largest_difference:e}");
        assert!(largest_difference < 2e-6, "largest difference: {largest_difference:e}");

        let input = [0.25, -0.5, 0.75, -1.0];
        let mut output = Vec::new();
        Resampler::new(16_000, 16_000).push(&input, &mut output);
        assert_eq!(output, input);
    }
}
