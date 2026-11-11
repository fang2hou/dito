use std::sync::Arc;

use dito_core::i18n::{Lang, Tr};
use gpui_kit::*;

use crate::state::AppState;
use crate::theme::{self, Palette};
use crate::ui;

pub struct Ctx {
    pub p: Palette,
    pub lang: Lang,
}

impl Ctx {
    pub fn new(st: &Arc<AppState>, cx: &App) -> Self {
        let cfg = st.cfg.get();
        let lang = cfg.general.lang();
        ui::set_ui_language(lang);
        Self { p: theme::palette(cfg.general.appearance, cx), lang }
    }

    pub fn t(&self, message: Tr) -> SharedString {
        SharedString::from(self.lang.tr(message))
    }
}
