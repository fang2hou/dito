//! The cloud polish form (20.05.7 to 20.05.10): the inputs that show the configuration in use, and the code that draws
//! them under the selector, with the parameter block and the footer.

use std::ops::Index;

use dito_core::config::{CloudPolishConfiguration, CloudPolishConfigurations, CloudPolishTarget, CloudPolishTemplate};
use dito_core::i18n::{Lang, Tr};
use dito_core::secrets::SecretStore;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::page::ModelsPage;
use super::polish_field_rules::CloudPolishField;
use crate::assets::Ic;
use crate::services;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, BtnKind, CircleSize, DialogAnswer, InputState, ParamField, TestState, cbk, cbk_idx, text};

/// The inputs of the cloud polish form: one per [`CloudPolishField`], showing the configuration in use. The inputs
/// outlive a selection: choosing another configuration puts its values into the same inputs.
pub(super) struct CloudPolishForm {
    inputs: [Entity<InputState>; 9],
    /// Id of the configuration the inputs show.
    shown: Option<String>,
    /// What the placeholders were written for: they depend on the template and the language.
    placeholders: Option<(CloudPolishTemplate, Lang)>,
}

impl CloudPolishForm {
    pub(super) fn new(window: &mut Window, cx: &mut Context<ModelsPage>) -> Self {
        Self {
            inputs: CloudPolishField::ALL.map(|field| ui::new_input(window, cx, field == CloudPolishField::Key)),
            shown: None,
            placeholders: None,
        }
    }

    /// The id of the configuration the inputs show.
    pub(super) fn shown(&self) -> Option<&str> {
        self.shown.as_deref()
    }

    pub(super) fn text(&self, field: CloudPolishField, cx: &App) -> String {
        self[field].read(cx).value().to_string()
    }

    /// Whether the text of `field` is wrong for its setting.
    pub(super) fn invalid(&self, field: CloudPolishField, cx: &App) -> bool {
        field.invalid(&self.text(field, cx))
    }

    /// Make the inputs show `active`, the configuration in use: its values when it is not the one they show already,
    /// and the placeholders of its template. Setting a value is not an edit, so nothing is saved by this.
    pub(super) fn follow(
        &mut self,
        active: Option<&CloudPolishConfiguration>,
        lang: Lang,
        secrets: &dyn SecretStore,
        window: &mut Window,
        cx: &mut Context<ModelsPage>,
    ) {
        let id = active.map(|config| config.id.as_str());
        if self.shown.as_deref() != id {
            for field in CloudPolishField::ALL {
                let value = match (active, field) {
                    (Some(config), CloudPolishField::Key) => secrets.get(&config.account()).unwrap_or_default(),
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
                for field in CloudPolishField::ALL {
                    let placeholder = placeholder(field, template, lang);
                    self[field].update(cx, |input, cx| input.set_placeholder(placeholder, window, cx));
                }
            }
            self.placeholders = wanted;
        }
    }

    /// Show `name` in the name field, after it was emptied and focus left it.
    pub(super) fn set_name(&self, name: String, window: &mut Window, cx: &mut Context<ModelsPage>) {
        self[CloudPolishField::Name].update(cx, |input, cx| input.set_value(name, window, cx));
    }

    /// Put the cursor in the name field with all of its text selected: typing replaces the name.
    pub(super) fn edit_name(&self, window: &mut Window, cx: &mut Context<ModelsPage>) {
        self[CloudPolishField::Name].update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
    }
}

impl Index<CloudPolishField> for CloudPolishForm {
    type Output = Entity<InputState>;

    fn index(&self, field: CloudPolishField) -> &Self::Output {
        &self.inputs[CloudPolishField::ALL.iter().position(|f| *f == field).expect("every field is listed in ALL")]
    }
}

/// The placeholder of `field` for a configuration of `template`.
fn placeholder(field: CloudPolishField, template: CloudPolishTemplate, lang: Lang) -> &'static str {
    match (field, template) {
        (CloudPolishField::Name, _) => "",
        (CloudPolishField::Address, CloudPolishTemplate::Azure) => "https://…openai.azure.com/",
        (CloudPolishField::Address, CloudPolishTemplate::Bedrock) => "ap-northeast-1",
        (CloudPolishField::Address, CloudPolishTemplate::Compat) => "https://…",
        (CloudPolishField::Model, CloudPolishTemplate::Azure) => "gpt-6.1-terra",
        (CloudPolishField::Model, CloudPolishTemplate::Bedrock) => "anthropic.claude-sonnet-4-…",
        (CloudPolishField::Model, CloudPolishTemplate::Compat) => lang.tr(Tr::ModelsModelPlaceholder),
        (CloudPolishField::Key, _) => "••••••••",
        (CloudPolishField::Temperature | CloudPolishField::TopP | CloudPolishField::MaxTokens, _) => {
            lang.tr(Tr::ModelsCloudNumberPlaceholder)
        }
        (CloudPolishField::Effort, _) => lang.tr(Tr::ModelsEffortPlaceholder),
        (CloudPolishField::Extra, _) => lang.tr(Tr::ModelsExtraPlaceholder),
    }
}

/// What the tile row shows of each cloud polish configuration, in the order of the list: the name over the model, or
/// over the note that it has none yet.
pub(super) fn cloud_polish_config_tiles(configurations: &CloudPolishConfigurations, lang: Lang) -> Vec<ui::ConfigTile> {
    configurations
        .items
        .iter()
        .map(|item| {
            let model = item.target.model().trim();
            ui::ConfigTile {
                id: item.id.clone().into(),
                name: item.name.clone().into(),
                detail: if model.is_empty() { lang.tr(Tr::ModelsNoModel).into() } else { model.to_owned().into() },
                selected: item.id == configurations.selected,
            }
        })
        .collect()
}

impl ModelsPage {
    /// One tile per saved cloud polish configuration, and the plus button that adds one. A click selects, and the form
    /// follows; a drag reorders; the order is saved when the tile is dropped.
    pub(super) fn cloud_polish_tile_row(&self, c: &Ctx, cx: &mut Context<Self>) -> AnyElement {
        let cfg = self.st.cfg.get();
        let tiles = cloud_polish_config_tiles(&cfg.polish.engine.polish_configurations(c.lang), c.lang);
        ui::tile_row("cloud-polish-tiles", &c.p, &self.cloud_polish_tiles, tiles)
            .on_select(ui::cbk_id(cx, |this: &mut Self, id, _, cx| this.select_cloud_polish(id, cx)))
            .on_move(ui::cbk_move(cx, |this: &mut Self, id, to, _, cx| this.move_cloud_polish(id, to, cx)))
            .plus(ui::TilePlus {
                open: self.cloud_polish_menu_open,
                on_press: cbk(cx, |this: &mut Self, _, cx| {
                    this.cloud_polish_menu_open = !this.cloud_polish_menu_open;
                    cx.notify();
                }),
                menu: self.cloud_polish_menu_open.then(|| self.cloud_polish_add_menu(c, cx)),
            })
            .into_any_element()
    }

    /// The menu of the plus button: the three templates a new configuration can start from, then a copy of the
    /// configuration in use.
    fn cloud_polish_add_menu(&self, c: &Ctx, cx: &mut Context<Self>) -> AnyElement {
        let templates = CloudPolishTemplate::ALL
            .iter()
            .map(|&template| ui::AddEntry {
                label: c.t(template.label()),
                on_click: cbk(cx, move |this: &mut Self, window, cx| {
                    this.add_cloud_polish_from_template(template, window, cx)
                }),
            })
            .collect();
        let duplicate = ui::AddEntry {
            label: c.t(Tr::ModelsDuplicateConfiguration),
            on_click: cbk(cx, |this: &mut Self, window, cx| this.duplicate_cloud_polish(window, cx)),
        };
        let close = cbk(cx, |this: &mut Self, _, cx| {
            this.cloud_polish_menu_open = false;
            cx.notify();
        });
        ui::add_menu("cloud-polish-add-menu", &c.p, c.t(Tr::ModelsCreateFromTemplate), templates, duplicate, close)
            .into_any_element()
    }

    /// The form of the cloud polish configuration in use: its name and address, its model and credential, its
    /// parameters, and the footer with the connection test.
    pub(super) fn cloud_polish_view(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let col = div().w_full().flex().flex_col().gap(px(18.));
        let cfg = self.st.cfg.get();
        let configurations = cfg.polish.engine.polish_configurations(c.lang);
        let Some(active) = configurations.active() else { return col };
        let template = active.target.template();
        let form = &self.cloud_polish;

        let first_row = div()
            .w_full()
            .flex()
            .gap(px(16.))
            .child(
                ui::form_field(p, c.t(Tr::ModelsName), form_input(p, &form[CloudPolishField::Name]))
                    .w(px(216.))
                    .flex_none(),
            )
            .child(
                ui::form_field(p, c.t(address_label(template)), form_input(p, &form[CloudPolishField::Address]))
                    .flex_1()
                    .min_w(px(0.)),
            );
        let credential = match &active.target {
            CloudPolishTarget::Bedrock { profile, .. } => {
                ui::form_field(p, c.t(Tr::ModelsAwsProfile), self.cloud_polish_aws_profile_select(c, cx, profile))
            }
            CloudPolishTarget::Azure { .. } | CloudPolishTarget::Compat { .. } => {
                ui::form_field(p, c.t(Tr::ModelsApiKey), form_input(p, &form[CloudPolishField::Key]))
            }
        };
        let second_row = div()
            .w_full()
            .flex()
            .gap(px(16.))
            .child(
                ui::form_field(p, c.t(model_label(template)), form_input(p, &form[CloudPolishField::Model]))
                    .flex_1()
                    .min_w(px(0.)),
            )
            .child(credential.flex_1().min_w(px(0.)));
        let param = |label: Tr, field: CloudPolishField| {
            ParamField::new(c.t(label), &form[field]).invalid(form.invalid(field, cx))
        };
        let parameters = ui::parameter_block(
            p,
            c.t(Tr::ModelsParameters),
            c.t(Tr::ModelsReasoningHint),
            vec![
                param(Tr::ModelsTemperature, CloudPolishField::Temperature),
                param(Tr::ModelsTopP, CloudPolishField::TopP),
                param(Tr::ModelsMaxTokens, CloudPolishField::MaxTokens),
                param(Tr::ModelsReasoningEffort, CloudPolishField::Effort),
            ],
            vec![param(Tr::ModelsExtraParameters, CloudPolishField::Extra)],
        );
        let removable = configurations.items.len() > 1;
        col.child(first_row).child(second_row).child(parameters).child(self.cloud_polish_footer(c, cx, removable))
    }

    fn cloud_polish_aws_profile_select(&self, c: &Ctx, cx: &mut Context<Self>, current: &str) -> AnyElement {
        let p = &c.p;
        if self.aws_profiles.is_empty() {
            // Nothing to pick: the control stays visible but inert.
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
            cbk_idx(cx, |this: &mut Self, i, _, cx| this.pick_cloud_polish_aws_profile(i, cx)),
        )
        .into_any_element()
    }

    /// The footer: the state of the connection test on the left, on the right the trash and the test button.
    fn cloud_polish_footer(&self, c: &Ctx, cx: &mut Context<Self>, removable: bool) -> Div {
        let p = &c.p;
        let state = &self.cloud_polish_test;
        let mut status = div().flex_1().min_w(px(0.)).flex().items_center().gap(px(12.)).child(test_status(c, state));
        if let TestState::Failed(message) = state {
            status = status
                .child(text(ui::error_line(c.lang, message), 12., FontWeight::NORMAL, p.text).flex_1().min_w(px(0.)));
        }
        let trash = if removable {
            ui::icon_button_with(
                "cloud-polish-delete",
                p,
                Ic::Trash,
                false,
                CircleSize::Regular,
                cbk(cx, |this: &mut Self, window, cx| this.ask_delete_cloud_polish(window, cx)),
            )
        } else {
            ui::disabled_icon_button("cloud-polish-delete", p, Ic::Trash)
        };
        let enabled = *state != TestState::Running;
        div()
            .w_full()
            .pt(px(16.))
            .border_t_1()
            .border_color(p.border)
            .flex()
            .items_center()
            .gap(px(16.))
            .child(status)
            .child(
                div().flex().items_center().gap(px(10.)).child(trash).child(cloud_polish_test_button(c, cx, enabled)),
            )
    }

    /// The dialog that asks whether to delete the cloud polish configuration in use (20.05.12), while it is open.
    pub(super) fn cloud_polish_delete_dialog(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let name = self.deleting_cloud_polish.as_ref()?.name.clone();
        let c = Ctx::new(&self.st, cx);
        let body =
            text(dito_core::i18n::fmt::delete_configuration_body(c.lang, &name), 13., FontWeight::NORMAL, c.p.text)
                .line_height(px(20.))
                .into_any_element();
        Some(ui::confirm_dialog(
            window,
            &c.p,
            "cloud-polish-delete-dialog",
            &self.delete_dialog_focus,
            c.t(Tr::ModelsDeleteConfigurationTitle),
            vec![body],
            DialogAnswer {
                label: c.t(Tr::AsrEditorCancel),
                on_click: Some(cbk(cx, |this: &mut Self, window, cx| this.close_cloud_polish_delete(window, cx))),
            },
            DialogAnswer {
                label: c.t(Tr::PromptsRemoveConfirm),
                on_click: Some(cbk(cx, |this: &mut Self, window, cx| this.delete_cloud_polish(window, cx))),
            },
        ))
    }

    /// Choosing an AWS profile saves it into the Bedrock configuration the form shows.
    fn pick_cloud_polish_aws_profile(&mut self, index: usize, cx: &mut Context<Self>) {
        let (Some(name), Some(id)) =
            (self.aws_profiles.get(index).cloned(), self.cloud_polish.shown().map(str::to_owned))
        else {
            return;
        };
        let saved = services::try_save_config(&self.st, |cfg| {
            let lang = cfg.general.lang();
            if let Some(CloudPolishConfiguration { target: CloudPolishTarget::Bedrock { profile, .. }, .. }) =
                cfg.polish.engine.polish_configurations_mut(lang).get_mut(&id)
            {
                *profile = name;
            }
        });
        self.note_save(saved.is_ok());
        self.cloud_polish_test = TestState::Idle;
        cx.notify();
    }
}

/// Test state as a small tag: neutral while unchecked or running, green on success with the response time beside
/// it, and the danger tones on failure.
pub(in crate::settings) fn test_status(c: &Ctx, state: &TestState) -> Div {
    let p = &c.p;
    let (label, bg, fg, weight) = match state {
        TestState::Idle => (c.t(Tr::ModelsUnchecked), p.seg, p.muted, FontWeight::NORMAL),
        TestState::Running => (c.t(Tr::ModelsChecking), p.seg, p.muted, FontWeight::NORMAL),
        TestState::Ok(_) => (c.t(Tr::ModelsTestOk), p.model_running_bg, p.model_running_text, FontWeight::MEDIUM),
        TestState::Failed(_) => {
            (c.t(Tr::ModelsConnectionError), p.button_danger_bg, p.button_danger_text, FontWeight::MEDIUM)
        }
    };
    let tag = div()
        .flex_none()
        .px(px(10.))
        .py(px(4.))
        .rounded(px(6.))
        .bg(bg)
        .child(text(label, 12., weight, fg).line_height(relative(1.5)));
    let mut status = div().flex().items_center().gap(px(12.)).child(tag);
    if let TestState::Ok(ms) = state {
        status = status.child(
            div()
                .flex()
                .items_baseline()
                .gap(px(6.))
                .child(text(c.t(Tr::ModelsLatency), 12., FontWeight::NORMAL, p.muted))
                .child(text(ms.to_string(), 13., FontWeight::MEDIUM, p.text))
                .child(text("ms", 12., FontWeight::NORMAL, p.muted)),
        );
    }
    status
}

fn cloud_polish_test_button(c: &Ctx, cx: &mut Context<ModelsPage>, enabled: bool) -> AnyElement {
    let p = &c.p;
    if enabled {
        return ui::button(
            "cloud-polish-test",
            p,
            &c.t(Tr::ModelsTestConnection),
            BtnKind::Subtle,
            None,
            cbk(cx, |this: &mut ModelsPage, _, cx| this.run_cloud_polish_test(cx)),
        )
        .w(px(140.))
        .into_any_element();
    }
    div()
        .h(px(36.))
        .w(px(140.))
        .rounded_full()
        .bg(p.seg)
        .flex()
        .items_center()
        .justify_center()
        .child(text(c.t(Tr::ModelsTestConnection), 13., FontWeight::MEDIUM, p.muted))
        .into_any_element()
}

fn form_input(p: &Palette, state: &Entity<InputState>) -> impl IntoElement {
    ui::text_input(p, state, None).form()
}

/// The label of the first field of the form of a template.
fn address_label(template: CloudPolishTemplate) -> Tr {
    match template {
        CloudPolishTemplate::Azure => Tr::ModelsEndpoint,
        CloudPolishTemplate::Bedrock => Tr::ModelsRegion,
        CloudPolishTemplate::Compat => Tr::ModelsBaseUrl,
    }
}

/// The label of the model field of the form of a template.
fn model_label(template: CloudPolishTemplate) -> Tr {
    match template {
        CloudPolishTemplate::Azure => Tr::ModelsDeployment,
        CloudPolishTemplate::Bedrock => Tr::ModelsModelId,
        CloudPolishTemplate::Compat => Tr::ModelsModel,
    }
}

#[cfg(test)]
mod tests {
    use dito_core::config::CloudPolishConfigurations;
    use dito_core::i18n::{Lang, Tr};

    use super::{CloudPolishField, cloud_polish_config_tiles};

    #[test]
    fn the_tile_feed_follows_the_name_and_model_as_they_are_typed() {
        let mut list = CloudPolishConfigurations::new(Lang::En);
        list.select("compat");
        let compat = list.get_mut("compat").unwrap();
        assert!(CloudPolishField::Name.apply(compat, "Fast"));
        assert!(CloudPolishField::Model.apply(compat, "gpt-x"));
        let feed: Vec<_> = cloud_polish_config_tiles(&list, Lang::En)
            .into_iter()
            .map(|t| (t.id.to_string(), t.name.to_string(), t.detail.to_string(), t.selected))
            .collect();
        let none = Lang::En.tr(Tr::ModelsNoModel);
        assert_eq!(
            feed,
            [
                ("azure".into(), "Azure OpenAI".into(), none.into(), false),
                ("bedrock".into(), "Amazon Bedrock".into(), none.into(), false),
                ("compat".into(), "Fast".into(), "gpt-x".into(), true),
            ],
            "ordered as saved, the selected one marked, a missing model said so"
        );
    }

    #[test]
    fn a_model_of_blanks_counts_as_none() {
        let mut list = CloudPolishConfigurations::new(Lang::Ja);
        assert!(CloudPolishField::Model.apply(list.get_mut("azure").unwrap(), "   "));
        let feed = cloud_polish_config_tiles(&list, Lang::Ja);
        assert_eq!(feed[0].detail.to_string(), Lang::Ja.tr(Tr::ModelsNoModel));
    }
}
