//! Delivering text into the focused app, or to the clipboard when nothing editable has focus.

mod ax;
mod copy;
mod deliver;
mod focus;
mod focus_warmer;
mod insert_error;
mod keyboard;
mod pasteboard;
mod selection;
mod selection_source;
mod verify;

pub use deliver::MacSink;
pub use focus::{has_text_target, warm_focus};
pub use focus_warmer::MacFocusWarmer;
pub use insert_error::InsertError;
pub use pasteboard::{ClipboardSwap, copy_to_clipboard};
pub use selection::{FieldText, Selection, capture_selection};
pub use selection_source::MacSelection;
