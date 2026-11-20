//! macOS integration: global shortcut, permissions, text insertion, keychain, sounds.

pub mod audio;
pub mod hotkey;
pub mod insert;
pub mod keymap;
pub mod permissions;
pub mod system;

pub use audio::MacMusicMute;
pub use hotkey::{Action, HotkeyError, HotkeyService, Shortcut};
pub use insert::{FieldText, MacFocusWarmer, MacSelection, MacSink, Selection, capture_selection, warm_focus};
pub use permissions::{Permission, Status};
pub use system::{IntroVideo, KeychainSecrets, MacFrontmost, Player, local_utc_offset_secs, physical_memory_bytes};

/// For entry points the Tokio runtime calls. Its threads have no autorelease pool of their own, so what AppKit and
/// Foundation autorelease there is freed only when the thread exits: never on a runtime worker, and on a blocking
/// thread only once it has sat idle.
pub(crate) fn with_autorelease_pool<T>(work: impl FnOnce() -> T) -> T {
    objc2::rc::autoreleasepool(|_| work())
}
