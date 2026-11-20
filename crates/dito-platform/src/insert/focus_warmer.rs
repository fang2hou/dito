use dito_core::controller::FocusWarmer;

use super::warm_focus;

/// Warms the insertion target's accessibility tree as soon as recording starts.
pub struct MacFocusWarmer;

impl FocusWarmer for MacFocusWarmer {
    fn warm(&self) {
        warm_focus();
    }
}
