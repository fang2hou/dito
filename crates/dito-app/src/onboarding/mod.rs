//! First-run wizard. It only sets initial values; every setting stays editable in exactly one place afterwards.
//!
//! One `Onboarding` entity coordinates the whole wizard: it owns the state, the step transitions
//! ([`transition`]) and the window ([`window`]); each step's content lives in [`steps`], and the
//! pieces reused across steps in [`components`].

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
pub use window::{open, restart};
#[cfg(feature = "screenshots")]
pub use wizard::Onboarding;
#[cfg(not(feature = "screenshots"))]
use wizard::Onboarding;
use wizard::{PERMS, PreparePhase};
