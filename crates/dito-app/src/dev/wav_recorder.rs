use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::Result;
use dito_core::audio::AudioClip;
use dito_core::controller::{Recorder, RecorderError};

/// Plays a WAV file as "the microphone": whatever is recorded is that file. Lets the real engines be exercised
/// without a microphone, permissions or a speaker loop.
pub struct WavRecorder {
    clip: AudioClip,
    phase: AtomicU32,
}

impl WavRecorder {
    pub fn open(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
        let clip = AudioClip::from_wav(&bytes)
            .ok_or_else(|| anyhow::anyhow!("{} is not a 16-bit mono WAV", path.display()))?;
        Ok(Self { clip, phase: AtomicU32::new(0) })
    }
}

impl Recorder for WavRecorder {
    fn start(&self) -> Result<(), RecorderError> {
        self.phase.store(0, Ordering::Relaxed);
        Ok(())
    }
    fn stop(&self) -> AudioClip {
        self.clip.clone()
    }
    fn level(&self) -> f32 {
        let p = self.phase.fetch_add(1, Ordering::Relaxed) as f32;
        0.35 + 0.3 * (p * 0.35).sin().abs()
    }
}
