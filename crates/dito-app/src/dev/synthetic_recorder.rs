use std::sync::atomic::{AtomicU32, Ordering};

use dito_core::audio::AudioClip;
use dito_core::controller::{Recorder, RecorderError};

/// Stand-in microphone: a steady tone, so the mock pipeline always has speech to "recognize".
#[derive(Default)]
pub struct SyntheticRecorder {
    phase: AtomicU32,
}

impl Recorder for SyntheticRecorder {
    fn start(&self) -> Result<(), RecorderError> {
        self.phase.store(0, Ordering::Relaxed);
        Ok(())
    }
    fn stop(&self) -> AudioClip {
        AudioClip::new((0..32_000).map(|i| (i as f32 * 0.05).sin() * 0.3).collect())
    }
    fn level(&self) -> f32 {
        let p = self.phase.fetch_add(1, Ordering::Relaxed) as f32;
        0.35 + 0.3 * (p * 0.35).sin().abs()
    }
}
