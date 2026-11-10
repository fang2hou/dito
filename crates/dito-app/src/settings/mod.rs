//! The settings window: sidebar shell plus one view per page.

pub(crate) mod asr_editor;
mod asr_highlight;
mod configuration_panel;
mod ctx;
mod dictionary;
mod downloads_dir;
mod edge_fade;
mod general;
mod health;
mod history;
mod hud_slider;
mod models;
mod page;
mod page_layout;
mod page_query;
mod prompts;
mod query_state;
mod role;
mod shell;
mod sidebar;
mod stats;
mod status;
mod window;

pub use ctx::Ctx;
#[cfg(feature = "screenshots")]
pub use models::ModelsProbe;
pub use page::Page;
pub use page_layout::{content_width, page, page_fill};
#[cfg(feature = "screenshots")]
pub use shell::Settings;
pub use window::{global_state, is_open, open};
#[cfg(feature = "screenshots")]
pub use window::{handle, page_motion, shell_motion};
