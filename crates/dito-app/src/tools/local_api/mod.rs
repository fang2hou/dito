//! The local API tool: the loopback service and its MCP endpoint (`design:tools/local-api`, `design:tools/mcp`).

use std::sync::Arc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::state::AppState;

pub(crate) struct LocalApiPage {
    #[allow(dead_code)]
    st: Arc<AppState>,
}

impl LocalApiPage {
    pub(crate) fn new(st: Arc<AppState>, _window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { st }
    }

    /// The page became the visible one, or stopped being it.
    pub(crate) fn set_active(&mut self, _active: bool, _cx: &mut Context<Self>) {}
}

impl Render for LocalApiPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}
