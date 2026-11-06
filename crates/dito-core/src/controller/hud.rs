use crate::config::RecordMode;
use crate::error::ErrorKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorAction {
    Retry,
    OpenSettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HudState {
    Hidden,
    Listening { mode: RecordMode },
    Recognizing,
    Polishing,
    Inserted,
    Copied,
    Error { kind: ErrorKind, detail: String, action: ErrorAction },
}
