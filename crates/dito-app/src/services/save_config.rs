use dito_core::config::{Config, ConfigError};

use crate::state::AppState;

/// A failed write is logged and returned, and leaves the config as it was.
pub fn try_save_config(st: &AppState, update: impl FnOnce(&mut Config)) -> Result<(), ConfigError> {
    st.cfg.update(update).inspect_err(|e| tracing::warn!("could not save settings: {e:#}"))
}

/// [`try_save_config`] for a change whose failure the caller has nothing to say about.
pub fn save_config(st: &AppState, update: impl FnOnce(&mut Config)) {
    let _ = try_save_config(st, update);
}
