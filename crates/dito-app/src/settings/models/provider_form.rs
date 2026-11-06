use std::rc::Rc;

use dito_core::config::ProviderKind;
use dito_core::i18n::Tr;
use dito_core::provider::configured_asr::AsrProfiles;
use dito_core::secrets::Role;
use gpui_kit::component::input::InputState;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::inputs::{Field, RoleInputs};
use super::page::ModelsPage;
use crate::services;
use crate::settings::{Ctx, role};
use crate::theme::Palette;
use crate::ui::{self, BtnKind, TestState, cbk, text};

impl ModelsPage {
    pub(super) fn provider_tiles(&self, c: &Ctx, cx: &mut Context<Self>, role: Role, kind: ProviderKind) -> Div {
        let p = &c.p;
        let mut row = div().w_full().flex().gap(px(12.));
        if role == Role::Asr {
            let selected = self.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default().selected;
            for (i, name) in AsrProfiles::NAMES.iter().enumerate() {
                row = row.child(choice_tile(
                    p,
                    format!("asr-preset-{i}"),
                    format!("asr-preset-ring-{i}"),
                    selected == i,
                    SharedString::from(*name),
                    cx.listener(move |this, _, window, cx| {
                        services::save_config(&this.st, |c| {
                            c.asr.cloud_asr.get_or_insert_with(AsrProfiles::default).selected = i
                        });
                        this.show_active_preset(window, cx);
                        cx.notify();
                    }),
                ));
            }
            return row;
        }
        let role_index = role::index(role);
        for k in ProviderKind::ALL {
            row = row.child(choice_tile(
                p,
                format!("prov-{role_index}-{}", k.id()),
                format!("prov-ring-{role_index}-{}", k.id()),
                k == kind,
                c.lang.tr(k.label()).into(),
                cx.listener(move |this, _, _, cx| {
                    this.update_engine(role, move |e| e.provider.kind = k);
                    this.tests[role_index] = TestState::Idle;
                    cx.notify();
                }),
            ));
        }
        row
    }

    pub(super) fn provider_fields(&self, c: &Ctx, cx: &mut Context<Self>, role: Role) -> Div {
        let col = div().w_full().flex().flex_col().gap(px(18.));
        let inputs = &self.inputs[role::index(role)];
        if role == Role::Asr {
            return self.cloud_asr_fields(c, cx, inputs, col);
        }
        let cfg = self.st.cfg.get();
        let engine = role::engine(&cfg, role);
        let llm = role == Role::Polish;
        match engine.provider.kind {
            ProviderKind::Azure => azure_fields(c, llm, inputs, col),
            ProviderKind::Bedrock => {
                let profile = self.aws_profile_select(c, cx, role, &engine.provider.bedrock.profile);
                bedrock_fields(c, llm, inputs, profile, col)
            }
            ProviderKind::Compat => compat_fields(c, llm, inputs, col),
        }
    }

    fn cloud_asr_fields(&self, c: &Ctx, cx: &mut Context<Self>, inputs: &RoleInputs, col: Div) -> Div {
        let p = &c.p;
        col.child(ui::form_field(p, c.t(Tr::ModelsBaseUrl), form_input(p, &inputs[Field::CompatBase])))
            .child(ui::form_field(p, c.t(Tr::ModelsModelId), form_input(p, &inputs[Field::CompatModel])))
            .child(ui::form_field(p, c.t(Tr::ModelsApiKey), form_input(p, &inputs[Field::KeyCompat])))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(text(c.t(Tr::ModelsConnectionSettings), 14., FontWeight::MEDIUM, p.text))
                    .child(ui::button(
                        "asr-edit",
                        p,
                        &c.t(Tr::AsrEditorEdit),
                        BtnKind::Subtle,
                        None,
                        cbk(cx, |this: &mut Self, w, cx| this.open_editor(w, cx)),
                    )),
            )
    }

    fn aws_profile_select(&self, c: &Ctx, cx: &mut Context<Self>, role: Role, current: &str) -> AnyElement {
        let p = &c.p;
        if self.profiles.is_empty() {
            // Nothing to pick: the control stays visible but inert.
            return ui::select_disabled(p, c.t(Tr::ModelsNoProfiles)).into_any_element();
        }
        let options: Vec<SharedString> = self.profiles.iter().map(|s| SharedString::from(s.clone())).collect();
        let selected = self.profiles.iter().position(|n| n == current).unwrap_or(0);
        let (st, profiles) = (self.st.clone(), self.profiles.clone());
        ui::select(
            "bedrock-profile",
            p,
            &options,
            selected,
            self.select == Some("bedrock-profile"),
            None,
            self.toggle_select(cx, "bedrock-profile"),
            Rc::new(move |i, _, _| {
                if let Some(name) = profiles.get(i).cloned() {
                    services::save_config(&st, |cfg| role::engine_mut(cfg, role).provider.bedrock.profile = name);
                }
            }),
        )
        .into_any_element()
    }

    pub(super) fn test_footer(&self, c: &Ctx, cx: &mut Context<Self>, role: Role) -> Div {
        let p = &c.p;
        let state = &self.tests[role::index(role)];
        let mut left = div().flex_1().min_w(px(0.)).flex().flex_col().gap(px(6.)).child(test_status(c, role, state));
        if let TestState::Failed(message) = state {
            left = left.child(text(ui::error_line(c.lang, message), 12., FontWeight::NORMAL, p.text));
        }
        let enabled = self.test_ready(role) && *state != TestState::Running;
        div()
            .w_full()
            .pt(px(16.))
            .border_t_1()
            .border_color(p.border)
            .flex()
            .items_center()
            .gap(px(16.))
            .child(left)
            .child(test_button(c, cx, role, enabled))
    }

    /// Whether the role's connection is complete enough to test: a cloud ASR profile must validate and carry a key
    /// when its request template asks for one.
    fn test_ready(&self, role: Role) -> bool {
        if role != Role::Asr {
            return true;
        }
        let profiles = self.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
        dito_core::provider::configured_asr::validate(profiles.active()).is_ok()
            && (!profiles.active()["request"].to_string().contains("{{api_key}}")
                || !self.st.secrets.get(&profiles.account()).unwrap_or_default().trim().is_empty())
    }
}

/// Test state as a small tag: neutral while unchecked or running, green on success with the response time beside
/// it, and the danger tones on failure.
fn test_status(c: &Ctx, role: Role, state: &TestState) -> Div {
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
    if role == Role::Polish
        && let TestState::Ok(ms) = state
    {
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

fn test_button(c: &Ctx, cx: &mut Context<ModelsPage>, role: Role, enabled: bool) -> AnyElement {
    let p = &c.p;
    if enabled {
        return ui::button(
            SharedString::from(format!("test-{}", role::index(role))),
            p,
            &c.t(Tr::ModelsTestConnection),
            BtnKind::Subtle,
            None,
            cbk(cx, move |this: &mut ModelsPage, _, cx| this.run_test(role, cx)),
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

fn choice_tile(
    p: &Palette,
    tile_id: String,
    ring_id: String,
    on: bool,
    label: SharedString,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let card_id = ElementId::from(SharedString::from(tile_id));
    div()
        .id(card_id.clone())
        .flex_1()
        .h(px(56.))
        .px(px(18.))
        .flex()
        .items_center()
        .justify_between()
        .rounded(px(14.))
        .cursor_pointer()
        .bg(if on { p.accent_soft } else { p.card })
        .relative()
        .border_1()
        .border_color(ui::transparent())
        .on_hover(ui::hover_tracker(card_id.clone()))
        .child(ui::hover_fill_if(&card_id, Some(14.), ui::hex_alpha(p.seg, 0.6), !on))
        .child(ui::ring_overlay(SharedString::from(ring_id), on, 14., p.border_strong, p.accent))
        .on_click(on_click)
        .child(ui::choice_label(&card_id, p, label, 14., on))
        .child(ui::radio(p, on))
}

fn form_input(p: &Palette, state: &Entity<InputState>) -> impl IntoElement {
    ui::text_input(p, state, None).form()
}

fn wide_field(p: &Palette, label: SharedString, state: &Entity<InputState>) -> Div {
    ui::form_field(p, label, form_input(p, state)).flex_1().min_w(px(0.))
}

fn effort_slot(c: &Ctx, state: &Entity<InputState>) -> Div {
    div().w(px(200.)).flex_none().child(ui::form_field(&c.p, c.t(Tr::ModelsReasoningEffort), form_input(&c.p, state)))
}

fn reasoning_hint(c: &Ctx) -> Div {
    text(c.t(Tr::ModelsReasoningHint), 12., FontWeight::NORMAL, c.p.muted)
}

/// The model field row shared by providers with an API key: polish puts the reasoning effort beside
/// the model and the key below; dictation puts the key beside the model.
fn model_and_key(
    c: &Ctx,
    llm: bool,
    model: (SharedString, &Entity<InputState>),
    effort: &Entity<InputState>,
    key: &Entity<InputState>,
    col: Div,
) -> Div {
    let p = &c.p;
    let row = div().w_full().flex().gap(px(16.)).child(wide_field(p, model.0, model.1));
    if llm {
        col.child(row.child(effort_slot(c, effort))).child(reasoning_hint(c)).child(ui::form_field(
            p,
            c.t(Tr::ModelsApiKey),
            form_input(p, key),
        ))
    } else {
        col.child(row.child(ui::form_field(p, c.t(Tr::ModelsApiKey), form_input(p, key)).flex_1().min_w(px(0.))))
    }
}

fn azure_fields(c: &Ctx, llm: bool, inputs: &RoleInputs, col: Div) -> Div {
    let p = &c.p;
    let col = col.child(ui::form_field(p, c.t(Tr::ModelsEndpoint), form_input(p, &inputs[Field::AzureEndpoint])));
    model_and_key(
        c,
        llm,
        (c.t(Tr::ModelsDeployment), &inputs[Field::AzureDeployment]),
        &inputs[Field::EffortAzure],
        &inputs[Field::KeyAzure],
        col,
    )
}

fn compat_fields(c: &Ctx, llm: bool, inputs: &RoleInputs, col: Div) -> Div {
    let p = &c.p;
    let col = col.child(ui::form_field(p, c.t(Tr::ModelsBaseUrl), form_input(p, &inputs[Field::CompatBase])));
    model_and_key(
        c,
        llm,
        (c.t(Tr::NavModels), &inputs[Field::CompatModel]),
        &inputs[Field::EffortCompat],
        &inputs[Field::KeyCompat],
        col,
    )
}

fn bedrock_fields(c: &Ctx, llm: bool, inputs: &RoleInputs, profile: AnyElement, col: Div) -> Div {
    let p = &c.p;
    let mut row =
        div().w_full().flex().gap(px(16.)).child(wide_field(p, c.t(Tr::ModelsModelId), &inputs[Field::BedrockModel]));
    if llm {
        row = row.child(effort_slot(c, &inputs[Field::EffortBedrock]));
    }
    let mut col =
        col.child(ui::form_field(p, c.t(Tr::ModelsRegion), form_input(p, &inputs[Field::BedrockRegion]))).child(row);
    if llm {
        col = col.child(reasoning_hint(c));
    }
    col.child(ui::form_field(p, c.t(Tr::ModelsAwsProfile), profile))
}
