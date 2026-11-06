//! What the screenshot harness steers on the Models page.

use dito_core::secrets::Role;
use gpui_kit::*;

use crate::settings::asr_editor::EditorPreview;
use crate::ui;

use super::page::ModelsPage;

impl ModelsPage {
    pub(in crate::settings) fn preview(
        &mut self,
        role: Role,
        editor: Option<EditorPreview>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tab = role;
        let profiles = self.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
        self.show_preset_connection(&profiles, window, cx);

        self.editor = None;
        if let Some(preview) = editor {
            self.open_editor(window, cx);
            if let Some(view) = &self.editor {
                view.update(cx, |v, cx| v.preview(preview, window, cx));
            }
        }
        cx.notify();
    }

    /// The page's scope, for the screenshot harness.
    pub(in crate::settings) fn motion_scope(&self) -> ui::ScopeHandle {
        self.motion.handle()
    }
}
