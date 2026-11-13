//! The parameters of the local text model in use (`design:settings/local-models/parameters`).

use std::ops::Index;

use dito_core::config::LocalTextParams;
use dito_core::i18n::{Lang, Tr};
use gpui_kit::*;

use super::page::ModelsPage;
use super::text_field_rules::LocalTextField;
use crate::settings::Ctx;
use crate::ui::{self, InputState, ParamField};

pub(super) struct LocalTextForm {
    inputs: [Entity<InputState>; 4],
    shown: Option<String>,
    placeholders: Option<Lang>,
}

impl LocalTextForm {
    pub(super) fn new(window: &mut Window, cx: &mut Context<ModelsPage>) -> Self {
        Self { inputs: LocalTextField::ALL.map(|_| ui::new_input(window, cx, false)), shown: None, placeholders: None }
    }

    pub(super) fn shown(&self) -> Option<&str> {
        self.shown.as_deref()
    }

    pub(super) fn text(&self, field: LocalTextField, cx: &App) -> String {
        self[field].read(cx).value().to_string()
    }

    pub(super) fn invalid(&self, field: LocalTextField, cx: &App) -> bool {
        field.invalid(&self.text(field, cx))
    }

    /// Setting a value is not an edit, so nothing is saved by this.
    pub(super) fn follow(
        &mut self,
        model: &str,
        params: &LocalTextParams,
        lang: Lang,
        window: &mut Window,
        cx: &mut Context<ModelsPage>,
    ) {
        if self.shown.as_deref() != Some(model) {
            for field in LocalTextField::ALL {
                let value = field.show(params);
                self[field].update(cx, |input, cx| input.set_value(value, window, cx));
            }
            self.shown = Some(model.to_owned());
        }
        if self.placeholders != Some(lang) {
            for field in LocalTextField::ALL {
                let placeholder = lang.tr(Tr::ModelsLocalNumberPlaceholder);
                self[field].update(cx, |input, cx| input.set_placeholder(placeholder, window, cx));
            }
            self.placeholders = Some(lang);
        }
    }
}

impl Index<LocalTextField> for LocalTextForm {
    type Output = Entity<InputState>;

    fn index(&self, field: LocalTextField) -> &Self::Output {
        &self.inputs[LocalTextField::ALL.iter().position(|f| *f == field).expect("every field is listed in ALL")]
    }
}

impl ModelsPage {
    /// An empty field uses the value recommended for that model.
    pub(super) fn local_text_parameters(&self, c: &Ctx, cx: &App) -> Div {
        let cfg = self.st.cfg.get();
        let model = &cfg.text_model.local_model;
        let name = self.st.models.spec(model).map_or_else(|| model.clone(), |spec| spec.name.clone());
        let form = &self.local_text;
        let field = |label: Tr, field: LocalTextField| {
            ParamField::new(c.t(label), &form[field]).invalid(form.invalid(field, cx))
        };
        ui::parameter_block(
            &c.p,
            c.t(Tr::ModelsParameters),
            dito_core::i18n::fmt::local_text_hint(c.lang, &name),
            vec![
                field(Tr::ModelsTemperature, LocalTextField::Temperature),
                field(Tr::ModelsTopP, LocalTextField::TopP),
                field(Tr::ModelsTopK, LocalTextField::TopK),
                field(Tr::ModelsMaxTokens, LocalTextField::MaxTokens),
            ],
            Vec::new(),
        )
    }
}
