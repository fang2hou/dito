use std::sync::Arc;

use gpui_kit::*;

use crate::state::AppState;
use crate::ui;

/// Re-render `view` whenever the config changes.
pub fn watch_config<V: 'static>(st: &Arc<AppState>, cx: &mut Context<V>) {
    ui::notify_on_change(st.cfg.subscribe(), cx);
}
