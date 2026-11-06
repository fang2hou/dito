//! macOS integration: global shortcut, permissions, text insertion, keychain, sounds.

pub mod audio;
pub mod hotkey;
pub mod insert;
pub mod keymap;
pub mod permissions;
pub mod system;

pub use audio::MacMusicMute;
pub use hotkey::{Action, HotkeyError, HotkeyService, Shortcut};
pub use insert::{MacSink, warm_focus};
pub use permissions::{Permission, Status};
pub use system::{KeychainSecrets, MacFrontmost, Player, local_utc_offset_secs, physical_memory_bytes};
