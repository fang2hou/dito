//! Small OS integrations: frontmost app, keychain, audio playback, login item, locale and the HUD's native window.

mod accessibility;
mod app_icon;
mod frontmost;
mod hud_follow;
mod hud_window;
mod keychain;
mod locale;
mod login_item;
mod nap_guard;
mod player;
mod sysinfo;

pub use accessibility::reduce_motion;
pub use app_icon::{has_bundle_icon, set_app_icon};
pub use frontmost::MacFrontmost;
pub use hud_follow::{ErrorBubble, HudFollowState, PointerContact};
pub use hud_window::{HudNativeWindow, HudPointer};
pub use keychain::KeychainSecrets;
pub use locale::system_language;
pub use login_item::{launch_at_login_enabled, set_launch_at_login};
pub use nap_guard::NapGuard;
pub use player::Player;
pub use sysinfo::{local_utc_offset_secs, physical_memory_bytes};
