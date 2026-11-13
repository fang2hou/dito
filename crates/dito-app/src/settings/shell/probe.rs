//! What the screenshot harness reads and steers on the settings window.

use dito_core::config::Appearance;
use dito_core::secrets::Role;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::hud::StylePreview;
use crate::settings::asr_editor::EditorPreview;

use super::Settings;

impl Settings {
    pub fn asr_config_editor(&self, cx: &App) -> Entity<crate::settings::asr_editor::AsrEditor> {
        self.models.read(cx).editor.clone()
    }

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

    pub fn preview_prompts(&self, case: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.prompts.update(cx, |p, cx| p.preview(case, window, cx));
    }

    /// Opens the newest transcript as if it were still being recorded.
    pub fn preview_live_transcript(&self, tail: [&str; 3], window: &mut Window, cx: &mut Context<Self>) {
        self.transcription.update(cx, |t, cx| t.preview_live(tail, window, cx));
    }

    /// The transcription page follows this session instead of the app's own.
    pub fn follow_transcription(
        &self,
        handle: dito_core::transcription::TranscriptionHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.transcription.update(cx, |t, cx| t.follow(handle, window, cx));
    }

    /// Opens the newest transcript, as a click on its row does.
    pub fn preview_transcript(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.transcription.update(cx, |t, cx| t.open_newest(window, cx));
    }

    pub fn probe_wordbook(&self, case: &str, arg: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.wordbook.update(cx, |d, cx| d.probe(case, arg, window, cx));
    }

    pub fn wordbook_failure(&self, cx: &App) -> Option<&'static str> {
        self.wordbook.read(cx).failure_name()
    }

    pub fn probe_models(&self, case: &str, arg: &str, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.models.update(cx, |m, cx| m.probe(case, arg, text, window, cx));
    }

    pub fn models_probe(&self, window: &Window, cx: &App) -> crate::settings::ModelsProbe {
        self.models.read(cx).probe_state(window, cx)
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
