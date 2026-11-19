//! The window chrome the Settings and Tools windows share: the frame around the page, the nav
//! sidebar, the rail it collapses into, the brand health dot with the card it opens, and the single
//! window each of them lives in.

mod chrome;
mod frame;
mod health;
mod nav;
mod sidebar;
mod window;

pub(crate) use chrome::Chrome;
pub(crate) use frame::frame;
pub(crate) use nav::{NavHost, NavPage, RAIL_COLLAPSE, RAIL_EXPAND, WindowDestination, traffic_light_position};
pub(crate) use window::{ShellView, ShellWindow, open_shell_window};
