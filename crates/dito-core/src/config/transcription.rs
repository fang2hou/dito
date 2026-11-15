use serde::{Deserialize, Serialize};

use crate::store::Sources;

/// The default instruction for a generated meeting title: one emoji, so a generated title reads as generated, then
/// about ten characters. It is shown and edited in the transcription settings, so it is user data, not a prompt rule.
pub const DEFAULT_TITLE_TEMPLATE: &str = "会議の内容に合う絵文字を1つ選び、半角スペースのあとに10文字前後のタイトルを続けてください。\n例：📋 リリース計画の確認";

/// What a transcription session records, and how it names a meeting left untitled. At least one source is on; a file
/// with both off records the Mac's audio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TranscriptionSettings {
    /// Everything the Mac plays, such as the other side of a meeting.
    pub system: bool,
    /// The user's own voice. With both on, the microphone is a separate speaker; that is meant for headphones, since
    /// through speakers the microphone hears the other side again.
    pub microphone: bool,
    /// A meeting that ends with an empty title gets one from the text model.
    pub auto_title: bool,
    /// The instruction the text model follows for that title.
    pub title_template: String,
}

impl Default for TranscriptionSettings {
    fn default() -> Self {
        let Sources { system, microphone } = Sources::default();
        Self { system, microphone, auto_title: true, title_template: DEFAULT_TITLE_TEMPLATE.to_owned() }
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

    /// The instruction to send: the saved one, or the default when it was cleared.
    pub fn title_instruction(&self) -> &str {
        if self.title_template.trim().is_empty() { DEFAULT_TITLE_TEMPLATE } else { &self.title_template }
    }
}
