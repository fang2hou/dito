//! The cloud polish form and its inputs (`design:settings/cloud-models/configurations`,
//! `design:settings/cloud-models/failed-test`).

use std::ops::Index;

use dito_core::config::{CloudTextConfiguration, CloudTextTarget, CloudTextTemplate};
use dito_core::i18n::{Lang, Tr};
use dito_core::secrets::SecretStore;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::page::ModelsPage;
use super::text_field_rules::CloudTextField;
use crate::services;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, ErrorPlacement, FooterTrash, InputState, ParamField, TestButton, TestState, cbk, cbk_idx};

/// The inputs outlive a selection: choosing another configuration puts its values into the same inputs.
pub(super) struct CloudTextForm {
    inputs: [Entity<InputState>; 9],
    shown: Option<String>,
    placeholders: Option<(CloudTextTemplate, Lang)>,
}

impl CloudTextForm {
    pub(super) fn new(window: &mut Window, cx: &mut Context<ModelsPage>) -> Self {
        Self {
            inputs: CloudTextField::ALL.map(|field| ui::new_input(window, cx, field == CloudTextField::Key)),
            shown: None,
            placeholders: None,
        }
    }

    pub(super) fn shown(&self) -> Option<&str> {
        self.shown.as_deref()
    }

    pub(super) fn text(&self, field: CloudTextField, cx: &App) -> String {
        self[field].read(cx).value().to_string()
    }

    pub(super) fn invalid(&self, field: CloudTextField, cx: &App) -> bool {
        field.invalid(&self.text(field, cx))
    }

    /// Setting a value is not an edit, so nothing is saved by this.
    pub(super) fn follow(
        &mut self,
        active: Option<&CloudTextConfiguration>,
        lang: Lang,
        secrets: &dyn SecretStore,
        window: &mut Window,
        cx: &mut Context<ModelsPage>,
    ) {
        let id = active.map(|config| config.id.as_str());
        if self.shown.as_deref() != id {
            for field in CloudTextField::ALL {
                let value = match (active, field) {
                    (Some(config), CloudTextField::Key) => secrets.get(&config.account()).unwrap_or_default(),
                    (Some(config), _) => field.show(config),
                    (None, _) => String::new(),
                };
                self[field].update(cx, |input, cx| input.set_value(value, window, cx));
            }
            self.shown = id.map(str::to_owned);
        }
        let wanted = active.map(|config| (config.target.template(), lang));
        if self.placeholders != wanted {
            if let Some((template, lang)) = wanted {
                for field in CloudTextField::ALL {
                    let placeholder = placeholder(field, template, lang);
                    self[field].update(cx, |input, cx| input.set_placeholder(placeholder, window, cx));
                }
            }
            self.placeholders = wanted;
        }
    }

    pub(super) fn set_name(&self, name: String, window: &mut Window, cx: &mut Context<ModelsPage>) {
        self[CloudTextField::Name].update(cx, |input, cx| input.set_value(name, window, cx));
    }
}

impl Index<CloudTextField> for CloudTextForm {
    type Output = Entity<InputState>;

    fn index(&self, field: CloudTextField) -> &Self::Output {
        &self.inputs[CloudTextField::ALL.iter().position(|f| *f == field).expect("every field is listed in ALL")]
    }
}

fn placeholder(field: CloudTextField, template: CloudTextTemplate, lang: Lang) -> &'static str {
    match (field, template) {
        (CloudTextField::Name, _) => "",
        (CloudTextField::Address, CloudTextTemplate::Azure) => "https://…openai.azure.com/",
        (CloudTextField::Address, CloudTextTemplate::Bedrock) => "ap-northeast-1",
        (CloudTextField::Address, CloudTextTemplate::Compat) => "https://…",
        (CloudTextField::Model, CloudTextTemplate::Azure) => "gpt-6.1-terra",
        (CloudTextField::Model, CloudTextTemplate::Bedrock) => "anthropic.claude-sonnet-4-…",
        (CloudTextField::Model, CloudTextTemplate::Compat) => lang.tr(Tr::ModelsModelPlaceholder),
        (CloudTextField::Key, _) => "••••••••",
        (CloudTextField::Temperature | CloudTextField::TopP | CloudTextField::MaxTokens, _) => {
            lang.tr(Tr::ModelsCloudNumberPlaceholder)
        }
        (CloudTextField::Effort, _) => lang.tr(Tr::ModelsEffortPlaceholder),
        (CloudTextField::Extra, _) => lang.tr(Tr::ModelsExtraPlaceholder),
    }
}

impl ModelsPage {
    pub(super) fn cloud_text_view(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let col = div().w_full().flex().flex_col().gap(px(18.));
        let cfg = self.st.cfg.get();
        let configurations = cfg.text_model.text_configurations(c.lang);
        let Some(active) = configurations.active() else { return col };
        let template = active.target.template();
        let form = &self.cloud_text.form;

        let first_row = div()
            .w_full()
            .flex()
            .gap(px(16.))
            .child(
                ui::form_field(p, c.t(Tr::ModelsName), form_input(p, &form[CloudTextField::Name]))
                    .w(px(216.))
                    .flex_none(),
            )
            .child(
                ui::form_field(p, c.t(address_label(template)), form_input(p, &form[CloudTextField::Address]))
                    .flex_1()
                    .min_w(px(0.)),
            );
        let credential = match &active.target {
            CloudTextTarget::Bedrock { profile, .. } => {
                ui::form_field(p, c.t(Tr::ModelsAwsProfile), self.cloud_text_aws_profile_select(c, cx, profile))
            }
            CloudTextTarget::Azure { .. } | CloudTextTarget::Compat { .. } => {
                ui::form_field(p, c.t(Tr::ModelsApiKey), form_input(p, &form[CloudTextField::Key]))
            }
        };
        let second_row = div()
            .w_full()
            .flex()
            .gap(px(16.))
            .child(
                ui::form_field(p, c.t(model_label(template)), form_input(p, &form[CloudTextField::Model]))
                    .flex_1()
                    .min_w(px(0.)),
            )
            .child(credential.flex_1().min_w(px(0.)));
        let param = |label: Tr, field: CloudTextField| {
            ParamField::new(c.t(label), &form[field]).invalid(form.invalid(field, cx))
        };
        let parameters = ui::parameter_block(
            p,
            c.t(Tr::ModelsParameters),
            c.t(Tr::ModelsReasoningHint),
            vec![
                param(Tr::ModelsTemperature, CloudTextField::Temperature),
                param(Tr::ModelsTopP, CloudTextField::TopP),
                param(Tr::ModelsMaxTokens, CloudTextField::MaxTokens),
                param(Tr::ModelsReasoningEffort, CloudTextField::Effort),
            ],
            vec![param(Tr::ModelsExtraParameters, CloudTextField::Extra)],
        );
        let removable = configurations.items.len() > 1;
        col.child(first_row).child(second_row).child(parameters).child(self.cloud_text_footer(c, cx, removable))
    }

    fn cloud_text_aws_profile_select(&self, c: &Ctx, cx: &mut Context<Self>, current: &str) -> AnyElement {
        let p = &c.p;
        if self.aws_profiles.is_empty() {
            return ui::select_form_disabled(p, c.t(Tr::ModelsNoProfiles)).into_any_element();
        }
        let options: Vec<SharedString> = self.aws_profiles.iter().map(|s| SharedString::from(s.clone())).collect();
        let selected = self.aws_profiles.iter().position(|n| n == current).unwrap_or(0);
        ui::select_form(
            "cloud-polish-aws-profile",
            p,
            &options,
            selected,
            self.select == Some("cloud-polish-aws-profile"),
            self.toggle_select(cx, "cloud-polish-aws-profile"),
            cbk_idx(cx, |this: &mut Self, i, _, cx| this.pick_cloud_text_aws_profile(i, cx)),
        )
        .into_any_element()
    }

    fn cloud_text_footer(&self, c: &Ctx, cx: &mut Context<Self>, removable: bool) -> Div {
        let state = &self.cloud_text.test;
        ui::form_footer(
            &c.p,
            ui::test_summary(&c.p, c.lang, state, ErrorPlacement::Beside),
            FooterTrash {
                id: "cloud-polish-delete",
                tip: None,
                on_click: removable
                    .then(|| cbk(cx, |this: &mut Self, window, cx| this.ask_delete_cloud_text(window, cx))),
            },
            TestButton {
                id: "cloud-polish-test",
                label: c.t(Tr::ModelsTestConnection),
                width: Some(140.),
                on_click: (*state != TestState::Running)
                    .then(|| cbk(cx, |this: &mut Self, _, cx| this.run_cloud_text_test(cx))),
            },
            Vec::new(),
        )
    }

    fn pick_cloud_text_aws_profile(&mut self, index: usize, cx: &mut Context<Self>) {
        let (Some(name), Some(id)) =
            (self.aws_profiles.get(index).cloned(), self.cloud_text.form.shown().map(str::to_owned))
        else {
            return;
        };
        let saved = services::try_save_config(&self.st, |cfg| {
            let lang = cfg.general.lang();
            if let Some(CloudTextConfiguration { target: CloudTextTarget::Bedrock { profile, .. }, .. }) =
                cfg.text_model.text_configurations_mut(lang).get_mut(&id)
            {
                *profile = name;
            }
        });
        self.note_save(saved.is_ok());
        self.cloud_text.test = TestState::Idle;
        cx.notify();
    }
}

fn form_input(p: &Palette, state: &Entity<InputState>) -> impl IntoElement {
    ui::text_input(p, state, None).form()
}

fn address_label(template: CloudTextTemplate) -> Tr {
    match template {
        CloudTextTemplate::Azure => Tr::ModelsEndpoint,
        CloudTextTemplate::Bedrock => Tr::ModelsRegion,
        CloudTextTemplate::Compat => Tr::ModelsBaseUrl,
    }
}

fn model_label(template: CloudTextTemplate) -> Tr {
    match template {
        CloudTextTemplate::Azure => Tr::ModelsDeployment,
        CloudTextTemplate::Bedrock => Tr::ModelsModelId,
        CloudTextTemplate::Compat => Tr::ModelsModel,
    }
}
