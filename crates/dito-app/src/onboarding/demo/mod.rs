//! The setup wizard's "how to use" demo.
//!
//! The loop's clock is the element's own state (kept by gpui across frames under the element's id), so the caller
//! holds nothing and renders [`usage_demo`] every frame like any other element.
//!
//! The sequence follows the storyboard of the design file (`design:onboarding/usage-demo`).

mod clock;
mod timeline;
mod view;

#[cfg(feature = "screenshots")]
pub use clock::{Frozen, freeze};
pub use view::usage_demo;
