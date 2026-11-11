//! The dictation controller: key events in, HUD states out. One actor task owns the state of the current dictation, so
//! rapid key presses, cancels and finishing pipeline runs are serialized.

mod actor;
mod command;
mod env;
mod handle;
mod history;
mod hud;
mod progress;
mod recorder_error;

pub use actor::{MAX_RECORDING_DURATION, spawn};
pub use command::Command;
pub use env::{AppInfo, Delivery, Deps, Env, FocusWarmer, Frontmost, MusicMute, MuteSession, Recorder, Sink};
pub use handle::DictationHandle;
pub use hud::{ErrorAction, HudState};
pub use progress::ProcessingEstimate;
pub use recorder_error::RecorderError;
