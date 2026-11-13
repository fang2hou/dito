use serde::{Deserialize, Serialize};

use crate::store::Sources;

/// What a transcription session records. At least one source is on; a file with both off records the Mac's audio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TranscriptionSettings {
    /// Everything the Mac plays, such as the other side of a meeting.
    pub system: bool,
    /// The user's own voice. With both on, the microphone is a separate speaker; that is meant for headphones, since
    /// through speakers the microphone hears the other side again.
    pub microphone: bool,
}

impl Default for TranscriptionSettings {
    fn default() -> Self {
        let Sources { system, microphone } = Sources::default();
        Self { system, microphone }
    }
}

impl TranscriptionSettings {
    pub fn sources(&self) -> Sources {
        if self.system || self.microphone {
            Sources { system: self.system, microphone: self.microphone }
        } else {
            Sources::default()
        }
    }
}
