//! Global shortcut logic, independent of the OS event source so it can be unit-tested.

mod keys;
mod machine;
mod shortcut;
mod tap;

pub use keys::{KEYCODE_ESCAPE, Modifier, Mods};
pub use machine::{Action, Machine, Output, RawEvent};
pub use shortcut::Shortcut;
pub use tap::{HotkeyError, HotkeyService};
