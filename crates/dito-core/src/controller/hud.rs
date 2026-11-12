use crate::config::RecordMode;
use crate::error::ErrorKind;
use crate::intent::Intent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorAction {
    Retry,
    OpenSettings,
}

/// What started the run the HUD shows, which decides its palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunKind {
    #[default]
    Dictation,
    Command,
}

/// One button of the choice window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    /// Never `Intent::Cancel`.
    pub intent: Intent,
    /// 0 to 100, rounded.
    pub percent: u8,
}

/// Where a confirmed command result may be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultWrite {
    /// Over the selection it was made from.
    ReplaceSelection,
    /// At the cursor: nothing was selected.
    InsertAtCursor,
    /// Nowhere: the window offers Copy only (a reply never goes over the message it answers, and text that cannot be
    /// edited is never written over).
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
    /// Command mode: the instruction was not a command Dito can run. A notice without buttons; nothing ran.
    NotRecognized,
    /// Command mode: the intent is being decided.
    Thinking,
    /// Command mode: the tool is running.
    Working,
    /// The decision was not confident: up to three intents, most likely first.
    Choosing {
        choices: Vec<Choice>,
    },
    /// The instruction field is open.
    TypingInstruction,
    /// The tool needs text and nothing was selected.
    NeedsText,
    /// The instruction was a question; the window shows the answer and nothing is replaced.
    Answer {
        text: String,
    },
    /// The result waits for a click; `write` is what the window may do with it besides copying it.
    Confirm {
        text: String,
        write: ResultWrite,
    },
    /// A command's result is in place of the selection.
    Replaced,
}
