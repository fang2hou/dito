//! Delivering text into the focused app, or to the clipboard when nothing editable has focus.
//!
//! Delivery waits for the keyboard focus to settle, then drives the type, paste and clipboard
//! fallback from [`dito_core::insert`], verifying each live attempt through the target field's
//! selection before accepting it. Every failed step is visible in `tracing`.

mod ax;
mod deliver;
mod focus;
mod insert_error;
mod keyboard;
mod pasteboard;
mod verify;

pub use deliver::MacSink;
pub use focus::{has_text_target, warm_focus};
pub use insert_error::InsertError;
pub use pasteboard::copy_to_clipboard;
