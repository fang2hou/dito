use dito_core::config::Config;

use crate::state::AppState;

/// Apply a settings change and persist it; a failed write is logged and leaves the config as it was.
pub fn save_config(st: &AppState, update: impl FnOnce(&mut Config)) {
    if let Err(e) = st.cfg.update(update) {
        tracing::warn!("could not save settings: {e:#}");
    }
}
