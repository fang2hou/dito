//! Transcription of meetings: live audio (the Mac's output, the microphone, or both) is cut into utterances, each
//! utterance is recognized as it ends (and about every second while it goes on, for partial text that is shown but
//! never stored), utterances are grouped into paragraphs of a few sentences, and every paragraph is polished with the
//! paragraphs before it as read-only context. One actor owns the running session.

mod energy_vad;
mod live;
mod paragraphs;
mod prompt;
mod recognizer;
mod session;
mod source;
mod wordbook;

pub use energy_vad::EnergyDetectors;
pub use live::LiveText;
pub use paragraphs::{PARAGRAPH_GAP_MS, PARAGRAPH_MAX_CHARS, PARAGRAPH_MIN_CHARS};
pub use session::{SessionState, TranscriptionDeps, TranscriptionHandle, TranscriptionStatus, spawn};

pub use crate::store::{Sources, Speaker};

use crate::controller::RecorderError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSource {
    /// Everything the Mac plays, through a system audio tap.
    System,
    Microphone,
}

/// Receives each block of 16 kHz mono samples on an audio thread.
pub type ChunkSink = Box<dyn FnMut(&[f32]) + Send>;

/// Opens 16 kHz mono streams. The platform implements it.
pub trait LiveAudio: Send + Sync {
    /// `on_chunk` runs on an audio thread for every block. Dropping the returned input stops the stream and drops
    /// `on_chunk` before returning.
    fn open(&self, source: AudioSource, on_chunk: ChunkSink) -> Result<Box<dyn LiveInput>, RecorderError>;
}

pub trait LiveInput: Send {
    /// Why the stream stopped on its own (its device unplugged or changed), once it has. The session then turns the
    /// source off (`design:tools/recording`, 23.02.5).
    fn lost(&self) -> Option<String> {
        None
    }
}

/// One detected utterance. `start_sample` counts 16 kHz samples from the first sample the detector accepted.
#[derive(Debug, Clone, PartialEq)]
pub struct Speech {
    pub start_sample: u64,
    pub samples: Vec<f32>,
}

/// Cuts one stream into utterances. Input may arrive in blocks of any size.
pub trait VoiceDetector: Send {
    fn accept(&mut self, samples: &[f32]);
    /// Ends the stream: speech still in progress becomes an utterance.
    fn flush(&mut self);
    fn pop(&mut self) -> Option<Speech>;
    /// Speech has started and not yet ended, so an utterance is on its way.
    fn speaking(&self) -> bool;
}

/// Each audio source gets a detector of its own.
pub trait VoiceDetectors: Send + Sync {
    fn create(&self) -> Result<Box<dyn VoiceDetector>, String>;
}
