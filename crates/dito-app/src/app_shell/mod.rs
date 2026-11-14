//! The window chrome the Settings and Tools windows share: the nav sidebar, the rail it collapses
//! into, and the brand health dot with the card it opens.

mod chrome;
mod health;
mod nav;
mod sidebar;

pub(crate) use chrome::Chrome;
pub(crate) use health::overlay as health_overlay;
pub(crate) use nav::{NavHost, NavPage, traffic_light_position};
pub(crate) use sidebar::{rail_hover, sidebar};
