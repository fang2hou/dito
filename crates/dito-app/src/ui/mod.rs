//! Dito's UI kit facade plus the app-bound controls.

mod format;
mod tag;
mod test_state;
#[path = "../typefaces.rs"]
mod typefaces;

pub use dito_ui::*;
pub use format::{error_line, error_summary, fmt_bytes, fmt_eta, history_target_label};
pub use tag::tag;
pub use test_state::{ErrorPlacement, TestState, test_summary};
pub use typefaces::set_ui_language;
