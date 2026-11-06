//! What the screenshot harness reads and steers on the General page.

use gpui_kit::*;

use crate::hud::StylePreview;
use crate::ui;

use super::GeneralPage;

impl GeneralPage {
    pub(in crate::settings) fn hud_preview(&self, cx: &App) -> Option<Entity<StylePreview>> {
        self.hud_preview.clone().filter(|preview| preview.read(cx).active())
    }

    pub(in crate::settings) fn hud_height_bounds(&self, cx: &App) -> Bounds<Pixels> {
        self.hud_height.read(cx).bounds(cx)
    }

    pub(in crate::settings) fn open_select(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.open = Some(id);
        if id == "mic" && (!self.st.opts.mock || self.st.opts.real_mic) {
            self.devices = dito_audio::list_input_devices();
        }
        cx.notify();
    }

    /// The page's scope, for the screenshot harness.
    pub(in crate::settings) fn motion_scope(&self) -> ui::ScopeHandle {
        self.motion.handle()
    }
}
