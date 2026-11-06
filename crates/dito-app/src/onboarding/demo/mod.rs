//! The setup wizard's "how to use" demo: a key, an arrow, the real HUD capsule and the phase of the recording
//! that is happening, looping on its own.
//!
//! [`usage_demo`] is a plain element. The loop's clock is the element's own state (kept by gpui across frames
//! under the element's id), so the caller holds nothing: it renders the demo every frame like any other element and
//! passes the mode. The clock stops while the window is inactive and resumes from the same phase, restarts at 0
//! when the mode changes (the old picture cross-fades into the new one), and under Reduce Motion the loop is
//! replaced by one still: keyframe 2, the recording.
//!
//! The sequence follows the storyboard of the design file (22.09 Usage Demo).

mod clock;
mod timeline;
mod view;

#[cfg(feature = "screenshots")]
pub use clock::{Frozen, freeze};
pub use view::usage_demo;
