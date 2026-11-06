//! What the screenshot harness reads and steers on the settings window.

use dito_core::config::Appearance;
use dito_core::secrets::Role;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::hud::StylePreview;
use crate::settings::asr_editor::EditorPreview;

use super::Settings;

impl Settings {
    pub fn health_hover_probe(&self) -> (Option<Bounds<Pixels>>, bool) {
        (self.health_dot_bounds.get(), Self::health_hovered(&self.motion.handle()))
    }

    pub fn set_hud_appearance(&self, appearance: Appearance, cx: &mut App) {
        self.general.update(cx, |general, cx| general.set_hud_appearance(appearance, cx));
    }

    pub fn hud_preview(&self, cx: &App) -> Option<Entity<StylePreview>> {
        self.general.read(cx).hud_preview(cx)
    }

    pub fn hud_height_bounds(&self, cx: &App) -> Bounds<Pixels> {
        self.general.read(cx).hud_height_bounds(cx)
    }

    pub fn open_general_select(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.general.update(cx, |g, cx| g.open_select(id, cx));
    }

    pub fn preview_models(
        &self,
        role: Role,
        editor: Option<EditorPreview>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.models.update(cx, |m, cx| m.preview(role, editor, window, cx));
    }
}
