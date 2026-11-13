use crate::config::RecordMode;
use crate::error::ErrorKind;
use crate::intent::Intent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorAction {
    Retry,
    OpenSettings,
}

/// Palette selector for the HUD: gold for dictation, ocean blue for command mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunKind {
    #[default]
    Dictation,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    /// Never `Intent::Cancel`.
    pub intent: Intent,
    pub percent: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultWrite {
    ReplaceSelection,
    InsertAtCursor,
    /// Reply results and uneditable selections are never written over the source text.
    CopyOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HudState {
    Hidden,
    Listening {
        mode: RecordMode,
    },
    Transcribing,
    Polishing,
    Inserted,
    Copied,
    Error {
        kind: ErrorKind,
        detail: String,
        action: ErrorAction,
    },
    /// Command instruction routed to `Intent::None`; shows a transient notice without buttons.
    NotRecognized,
    Thinking,
    Working,
    Choosing {
        choices: Vec<Choice>,
    },
    TypingInstruction,
    NeedsText,
    Answer {
        text: String,
    },
    Confirm {
        text: String,
        write: ResultWrite,
    },
    Replaced,
}
