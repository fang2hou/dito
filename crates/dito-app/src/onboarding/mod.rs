//! First-run wizard. It only sets initial values; every setting stays editable in exactly one place afterwards.

mod components;
pub(crate) mod demo;
mod step;
mod steps;
mod transition;
mod window;
mod wizard;

use step::Step;
#[cfg(feature = "screenshots")]
pub use window::open_with_view;
pub(crate) use window::{insert_try, try_anchor};
pub use window::{is_open, open, restart};
#[cfg(feature = "screenshots")]
pub use wizard::Onboarding;
#[cfg(not(feature = "screenshots"))]
use wizard::Onboarding;
use wizard::{PERMS, PreparePhase};
