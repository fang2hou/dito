//! The dictation controller: key events in, HUD states out. One actor task owns the state of the current dictation, so
//! rapid key presses, cancels and finishing pipeline runs are serialized. Command mode runs through the same actor: a
//! second shortcut captures the selection, records an instruction, and a tool answers or edits the selected text.

mod actor;
mod command;
mod env;
mod handle;
mod history;
mod hud;
mod progress;
mod recorder_error;

pub use actor::{CHORD_HOLD, MAX_RECORDING_DURATION, spawn};
pub use command::Command;
pub use env::{
    AppInfo, CapturedSelection, Delivery, Deps, Env, FocusWarmer, Frontmost, MusicMute, MuteSession, Recorder,
    SelectionSource, Sink,
};
pub use handle::DictationHandle;
pub use hud::{Choice, ErrorAction, HudState, ResultWrite, RunKind};
pub use progress::{ProcessingEstimate, StepTimes};
pub use recorder_error::RecorderError;
