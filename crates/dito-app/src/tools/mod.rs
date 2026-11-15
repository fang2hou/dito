//! The Tools window: the same shell as Settings with the meeting tools in its sidebar.

mod shell;
pub(crate) mod transcription;
mod window;

#[cfg(feature = "screenshots")]
pub use shell::Tools;
#[cfg(feature = "screenshots")]
pub use window::{handle, page_motion, shell_motion};
pub use window::{is_open, open};

pub(crate) use window::bring_to_front;
