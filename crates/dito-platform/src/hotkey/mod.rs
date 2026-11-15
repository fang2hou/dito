//! Global shortcut logic, independent of the OS event source so it can be unit-tested.

mod keys;
mod machine;
mod shortcut;
mod tap;

/// Set in the user-data field of every key event dito posts ("DITO" in ASCII); the tap lets those pass unseen.
pub(crate) const SYNTHETIC_EVENT_MARK: i64 = 0x4449_544F;

pub use keys::{KEYCODE_ESCAPE, Modifier, Mods};
pub use machine::{Action, Machine, Output, RawEvent};
pub use shortcut::{Shortcut, command_shortcut, dictation_may_hand_over, dictation_shortcut, tools_shortcut};
pub use tap::{HotkeyError, HotkeyService};
