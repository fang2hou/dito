//! Global shortcut logic, independent of the OS event source so it can be unit-tested.
//!
//! A shortcut is a modifier key on its own (right Option, Fn, ...), a modifier with other
//! modifiers held (⌃ + 右⌥), or a key with modifiers. The state machine consumes raw key
//! events and emits [`Action`]s.

mod keys;
mod machine;
mod shortcut;
mod tap;

pub use keys::{KEYCODE_ESCAPE, Modifier, Mods};
pub use machine::{Action, Machine, Output, RawEvent};
pub use shortcut::Shortcut;
pub use tap::{HotkeyError, HotkeyService};
