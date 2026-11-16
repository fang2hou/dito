//! The settings window: sidebar shell plus one view per page.

pub(crate) mod asr_editor;
mod asr_highlight;
mod configuration_panel;
mod ctx;
mod downloads_dir;
mod edge_fade;
mod general;
mod history;
mod hud_slider;
mod models;
mod page;
mod page_layout;
mod page_query;
mod prompts;
mod query_state;
pub(crate) mod role;
mod shell;
mod stats;
mod status;
mod window;
mod wordbook;

pub use ctx::Ctx;
pub use edge_fade::{Edge, edge_fade};
#[cfg(feature = "screenshots")]
pub use models::ModelsProbe;
pub use page::Page;
pub use page_layout::{content_width, page, page_fill};
#[cfg(feature = "screenshots")]
pub use shell::Settings;
pub use window::handle;
pub use window::{global_state, is_open, open, open_text_model, open_vad_models};
#[cfg(feature = "screenshots")]
pub use window::{page_motion, shell_motion};

pub(crate) use window::bring_to_front;
