//! The Tools window: the same shell as Settings with the meeting tools in its sidebar.

pub(crate) mod chat;
pub(crate) mod local_api;
mod shell;
pub(crate) mod transcription;
mod window;

#[cfg(feature = "screenshots")]
pub(crate) use local_api::LocalApiPose;
#[cfg(feature = "screenshots")]
pub use shell::Tools;
#[cfg(feature = "screenshots")]
pub(crate) use shell::ToolsPage;
#[cfg(feature = "screenshots")]
pub use window::{handle, page_motion, shell_motion};
pub use window::{is_open, open};

pub(crate) use window::bring_to_front;
