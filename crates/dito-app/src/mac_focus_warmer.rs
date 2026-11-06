use dito_core::controller::FocusWarmer;

/// Warms the insertion target's accessibility tree as soon as recording starts.
pub struct MacFocusWarmer;

impl FocusWarmer for MacFocusWarmer {
    fn warm(&self) {
        dito_platform::warm_focus();
    }
}
