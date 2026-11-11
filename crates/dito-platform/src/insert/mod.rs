//! Delivering text into the focused app, or to the clipboard when nothing editable has focus.

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
