//! A level-based detector for builds without the on-device runtimes (`--mock`) and for tests. Real sessions use
//! Silero VAD.

use super::{Speech, VoiceDetector, VoiceDetectors};

const FRAME: usize = 480; // 30 ms
const THRESHOLD: f32 = 0.01;
const HANGOVER_FRAMES: usize = 17; // about 0.5 s of silence ends an utterance
const MIN_SPEECH_FRAMES: usize = 8;
const MAX_SPEECH_FRAMES: usize = 667; // 20 s

#[derive(Default)]
pub struct EnergyDetectors;

impl VoiceDetectors for EnergyDetectors {
    fn create(&self) -> Result<Box<dyn VoiceDetector>, String> {
        Ok(Box::<EnergyVad>::default())
    }
}

#[derive(Default)]
struct EnergyVad {
    pending: Vec<f32>,
    /// Samples accepted in whole frames so far.
    position: u64,
    speech: Option<(u64, Vec<f32>)>,
    quiet_frames: usize,
    ready: std::collections::VecDeque<Speech>,
}

impl EnergyVad {
    fn end_utterance(&mut self) {
        if let Some((start, mut samples)) = self.speech.take() {
            let trailing = self.quiet_frames * FRAME;
            samples.truncate(samples.len().saturating_sub(trailing));
            if samples.len() >= MIN_SPEECH_FRAMES * FRAME {
                self.ready.push_back(Speech { start_sample: start, samples });
            }
        }
        self.quiet_frames = 0;
    }
}

impl VoiceDetector for EnergyVad {
    fn accept(&mut self, samples: &[f32]) {
        self.pending.extend_from_slice(samples);
        let whole = self.pending.len() / FRAME * FRAME;
        let frames: Vec<f32> = self.pending.drain(..whole).collect();
        for frame in frames.chunks(FRAME) {
            let rms = (frame.iter().map(|s| s * s).sum::<f32>() / FRAME as f32).sqrt();
            let loud = rms >= THRESHOLD;
            match &mut self.speech {
                Some((_, buf)) => {
                    buf.extend_from_slice(frame);
                    self.quiet_frames = if loud { 0 } else { self.quiet_frames + 1 };
                    if self.quiet_frames >= HANGOVER_FRAMES || buf.len() >= MAX_SPEECH_FRAMES * FRAME {
                        self.end_utterance();
                    }
                }
                None if loud => self.speech = Some((self.position, frame.to_vec())),
                None => {}
            }
            self.position += FRAME as u64;
        }
    }

    fn flush(&mut self) {
        self.end_utterance();
    }

    fn pop(&mut self) -> Option<Speech> {
        self.ready.pop_front()
    }

    fn speaking(&self) -> bool {
        self.speech.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loud_stretches_become_utterances_with_their_start() {
        let mut vad = EnergyDetectors.create().unwrap();
        let tone =
            |secs: f32| (0..(16_000. * secs) as usize).map(|i| (i as f32 * 0.05).sin() * 0.3).collect::<Vec<_>>();
        vad.accept(&vec![0.0; 16_000]);
        vad.accept(&tone(1.0));
        vad.accept(&vec![0.0; 16_000]);
        vad.accept(&tone(0.5));
        vad.flush();
        let first = vad.pop().unwrap();
        assert_eq!(first.start_sample, 15_840, "the frame the tone starts in");
        assert!((first.samples.len() as i64 - 16_000).abs() <= FRAME as i64);
        assert!(vad.pop().is_some(), "speech still running at the end is flushed");
        assert!(vad.pop().is_none());
    }
}
