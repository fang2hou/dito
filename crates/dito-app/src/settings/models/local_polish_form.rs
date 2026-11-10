//! The parameters of the local polish model in use (20.04.2): the inputs that show them, and the block under the model
//! list that draws them.

use std::ops::Index;

use dito_core::config::LocalParams;
use dito_core::i18n::{Lang, Tr};
use gpui_kit::*;

use super::page::ModelsPage;
use super::polish_field_rules::LocalPolishField;
use crate::settings::Ctx;
use crate::ui::{self, InputState, ParamField};

/// The inputs of the parameter block under the local polish models: one per [`LocalPolishField`], showing the model
/// in use.
pub(super) struct LocalPolishForm {
    inputs: [Entity<InputState>; 4],
    /// Id of the model whose parameters the inputs show.
    shown: Option<String>,
    placeholders: Option<Lang>,
}

impl LocalPolishForm {
    pub(super) fn new(window: &mut Window, cx: &mut Context<ModelsPage>) -> Self {
        Self {
            inputs: LocalPolishField::ALL.map(|_| ui::new_input(window, cx, false)),
            shown: None,
            placeholders: None,
        }
    }

    /// The id of the model whose parameters the inputs show.
    pub(super) fn shown(&self) -> Option<&str> {
        self.shown.as_deref()
    }

    pub(super) fn text(&self, field: LocalPolishField, cx: &App) -> String {
        self[field].read(cx).value().to_string()
    }

    /// Whether the text of `field` is wrong for its setting.
    pub(super) fn invalid(&self, field: LocalPolishField, cx: &App) -> bool {
        field.invalid(&self.text(field, cx))
    }

    /// Make the inputs show the `params` of `model`, the local polish model in use, unless they show it already.
    /// Setting a value is not an edit, so nothing is saved by this.
    pub(super) fn follow(
        &mut self,
        model: &str,
        params: &LocalParams,
        lang: Lang,
        window: &mut Window,
        cx: &mut Context<ModelsPage>,
    ) {
        if self.shown.as_deref() != Some(model) {
            for field in LocalPolishField::ALL {
                let value = field.show(params);
                self[field].update(cx, |input, cx| input.set_value(value, window, cx));
            }
            self.shown = Some(model.to_owned());
        }
        if self.placeholders != Some(lang) {
            for field in LocalPolishField::ALL {
                let placeholder = lang.tr(Tr::ModelsLocalNumberPlaceholder);
                self[field].update(cx, |input, cx| input.set_placeholder(placeholder, window, cx));
            }
            self.placeholders = Some(lang);
        }
    }
}

impl Index<LocalPolishField> for LocalPolishForm {
    type Output = Entity<InputState>;

    fn index(&self, field: LocalPolishField) -> &Self::Output {
        &self.inputs[LocalPolishField::ALL.iter().position(|f| *f == field).expect("every field is listed in ALL")]
    }
}

impl ModelsPage {
    /// The parameters of the local polish model in use, under the model list. An empty field uses the value
    /// recommended for that model.
    pub(super) fn local_polish_parameters(&self, c: &Ctx, cx: &App) -> Div {
        let cfg = self.st.cfg.get();
        let model = &cfg.polish.engine.local_model;
        let name = self.st.models.spec(model).map_or_else(|| model.clone(), |spec| spec.name.clone());
        let form = &self.local_polish;
        let field = |label: Tr, field: LocalPolishField| {
            ParamField::new(c.t(label), &form[field]).invalid(form.invalid(field, cx))
        };
        ui::parameter_block(
            &c.p,
            c.t(Tr::ModelsParameters),
            dito_core::i18n::fmt::local_polish_hint(c.lang, &name),
            vec![
                field(Tr::ModelsTemperature, LocalPolishField::Temperature),
                field(Tr::ModelsTopP, LocalPolishField::TopP),
                field(Tr::ModelsTopK, LocalPolishField::TopK),
                field(Tr::ModelsMaxTokens, LocalPolishField::MaxTokens),
            ],
            Vec::new(),
        )
    }
}
