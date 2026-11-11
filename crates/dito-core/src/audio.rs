/// Mono PCM at 16 kHz, the format every engine consumes.
pub const SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioClip {
    pub samples: Vec<f32>,
}

impl AudioClip {
    pub fn new(samples: Vec<f32>) -> Self {
        Self { samples }
    }

    pub fn silence(seconds: f32) -> Self {
        Self { samples: vec![0.0; (SAMPLE_RATE as f32 * seconds) as usize] }
    }

    pub fn duration_ms(&self) -> u64 {
        self.samples.len() as u64 * 1000 / SAMPLE_RATE as u64
    }

    pub fn rms(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let sum: f32 = self.samples.iter().map(|s| s * s).sum();
        (sum / self.samples.len() as f32).sqrt()
    }

    pub fn peak(&self) -> f32 {
        self.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()))
    }

    /// The 120ms floor keeps accidental key taps out while still admitting single-syllable words, which land at
    /// 150–300ms. A decent peak counts as speech even when lead-in silence dilutes the whole-clip RMS.
    pub fn is_effectively_silent(&self) -> bool {
        self.duration_ms() < 120 || (self.rms() < 0.003 && self.peak() < 0.01)
    }

    pub fn to_wav(&self) -> Vec<u8> {
        let data_len = (self.samples.len() * 2) as u32;
        let mut out = Vec::with_capacity(44 + data_len as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
        out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for s in &self.samples {
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    pub fn from_wav(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return None;
        }
        let mut pos = 12;
        while pos + 8 <= bytes.len() {
            let id = &bytes[pos..pos + 4];
            let len = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
            if id == b"data" {
                let end = (pos + 8 + len).min(bytes.len());
                let samples = bytes[pos + 8..end]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| i16::from_le_bytes(*c) as f32 / i16::MAX as f32)
                    .collect();
                return Some(Self { samples });
            }
            pos += 8 + len + (len & 1);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_round_trip_keeps_length_and_shape() {
        let clip = AudioClip::new((0..1600).map(|i| (i as f32 / 50.0).sin() * 0.5).collect());
        let back = AudioClip::from_wav(&clip.to_wav()).unwrap();
        assert_eq!(back.samples.len(), clip.samples.len());
        assert!((back.samples[400] - clip.samples[400]).abs() < 1e-3);
    }

    #[test]
    fn silence_and_short_clips_are_flagged() {
        assert!(AudioClip::silence(1.0).is_effectively_silent());
        assert!(AudioClip::new(vec![0.5; 100]).is_effectively_silent());
        assert!(!AudioClip::new(vec![0.3; 16_000]).is_effectively_silent());
    }

    #[test]
    fn single_syllable_clips_are_not_flagged() {
        assert!(!AudioClip::new(vec![0.3; 3_200]).is_effectively_silent());
        // A short word wrapped in lead-in silence: low whole-clip RMS, healthy peak.
        let mut samples = vec![0.0f32; 2_400];
        samples.extend(vec![0.4; 1_600]);
        samples.extend(vec![0.0f32; 2_400]);
        assert!(!AudioClip::new(samples).is_effectively_silent());
        // Near-silence with the same shape stays rejected.
        let mut samples = vec![0.0f32; 2_400];
        samples.extend(vec![0.004; 1_600]);
        samples.extend(vec![0.0f32; 2_400]);
        assert!(AudioClip::new(samples).is_effectively_silent());
    }
}
