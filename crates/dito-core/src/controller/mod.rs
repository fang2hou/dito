//! The dictation controller: key events in, HUD states out.
//!
//! One actor task owns the whole session so that rapid key presses, cancels and finishing jobs are
//! serialized. Platform pieces (microphone, text delivery, frontmost app) are injected as traits.

mod actor;
mod command;
mod env;
mod handle;
mod history;
mod hud;
mod recorder_error;

pub use actor::{MAX_RECORDING_DURATION, spawn};
pub use command::Command;
pub use env::{AppInfo, Delivery, Deps, Env, FocusWarmer, Frontmost, MusicMute, MuteSession, Recorder, Sink};
pub use handle::DictationHandle;
pub use hud::{ErrorAction, HudState};
pub use recorder_error::RecorderError;
