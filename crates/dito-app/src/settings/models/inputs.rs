use std::ops::Index;

use dito_core::config::{EngineConfig, ProviderKind, ProviderSettings};
use dito_core::i18n::{Lang, Tr};
use dito_core::secrets::{Role, account};
use gpui_kit::component::input::InputState;
use gpui_kit::*;

use super::page::ModelsPage;
use crate::settings::role;
use crate::state::AppState;
use crate::ui;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    AzureEndpoint,
    AzureDeployment,
    BedrockRegion,
    BedrockModel,
    CompatBase,
    CompatModel,
    EffortAzure,
    EffortBedrock,
    EffortCompat,
    KeyAzure,
    KeyCompat,
}

impl Field {
    pub(super) const ALL: [Field; 11] = [
        Self::AzureEndpoint,
        Self::AzureDeployment,
        Self::BedrockRegion,
        Self::BedrockModel,
        Self::CompatBase,
        Self::CompatModel,
        Self::EffortAzure,
        Self::EffortBedrock,
        Self::EffortCompat,
        Self::KeyAzure,
        Self::KeyCompat,
    ];

    fn masked(self) -> bool {
        self.key_provider().is_some()
    }

    pub(super) fn key_provider(self) -> Option<ProviderKind> {
        match self {
            Self::KeyAzure => Some(ProviderKind::Azure),
            Self::KeyCompat => Some(ProviderKind::Compat),
            _ => None,
        }
    }

    fn placeholder(self, lang: Lang) -> &'static str {
        match self {
            Self::AzureEndpoint => "https://…openai.azure.com/",
            Self::AzureDeployment => "gpt-6.1-terra",
            Self::BedrockRegion => "ap-northeast-1",
            Self::BedrockModel => "anthropic.claude-sonnet-4-…",
            Self::CompatBase => "https://…",
            Self::CompatModel => lang.tr(Tr::ModelsModelPlaceholder),
            Self::EffortAzure | Self::EffortBedrock | Self::EffortCompat => lang.tr(Tr::ModelsEffortPlaceholder),
            Self::KeyAzure | Self::KeyCompat => "••••••••",
        }
    }

    fn initial_value(self, engine: &EngineConfig, st: &AppState) -> String {
        let provider = &engine.provider;
        let key = |kind| st.secrets.get(&account(Role::Polish, kind)).unwrap_or_default();
        match self {
            Self::AzureEndpoint => provider.azure.endpoint.clone(),
            Self::AzureDeployment => provider.azure.deployment.clone(),
            Self::BedrockRegion => provider.bedrock.region.clone(),
            Self::BedrockModel => provider.bedrock.model_id.clone(),
            Self::CompatBase => provider.compat.base_url.clone(),
            Self::CompatModel => provider.compat.model.clone(),
            Self::EffortAzure => provider.azure.reasoning_effort.as_str().to_string(),
            Self::EffortBedrock => provider.bedrock.reasoning_effort.as_str().to_string(),
            Self::EffortCompat => provider.compat.reasoning_effort.as_str().to_string(),
            Self::KeyAzure => key(ProviderKind::Azure),
            Self::KeyCompat => key(ProviderKind::Compat),
        }
    }

    /// Store an edited value into the provider settings; key fields live in the secret store instead.
    pub(super) fn write(self, provider: &mut ProviderSettings, value: String) {
        match self {
            Self::EffortAzure => provider.azure.reasoning_effort = value.into(),
            Self::EffortBedrock => provider.bedrock.reasoning_effort = value.into(),
            Self::EffortCompat => provider.compat.reasoning_effort = value.into(),
            Self::AzureEndpoint => provider.azure.endpoint = value,
            Self::AzureDeployment => provider.azure.deployment = value,
            Self::BedrockRegion => provider.bedrock.region = value,
            Self::BedrockModel => provider.bedrock.model_id = value,
            Self::CompatBase => provider.compat.base_url = value,
            Self::CompatModel => provider.compat.model = value,
            Self::KeyAzure | Self::KeyCompat => {}
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|f| *f == self).expect("every field is listed in ALL")
    }
}

/// One text input per provider field of the polish engine; speech is edited in the cloud ASR editor instead.
pub(super) struct ProviderInputs([Entity<InputState>; 11]);

impl ProviderInputs {
    pub(super) fn new(st: &AppState, window: &mut Window, cx: &mut Context<ModelsPage>) -> Self {
        let cfg = st.cfg.get();
        let engine = role::engine(&cfg, Role::Polish);
        let lang = cfg.general.lang();
        Self(Field::ALL.map(|field| {
            make_input(window, cx, field.placeholder(lang), &field.initial_value(engine, st), field.masked())
        }))
    }
}

impl Index<Field> for ProviderInputs {
    type Output = Entity<InputState>;

    fn index(&self, field: Field) -> &Self::Output {
        &self.0[field.index()]
    }
}

fn make_input(
    window: &mut Window,
    cx: &mut Context<ModelsPage>,
    placeholder: &str,
    value: &str,
    masked: bool,
) -> Entity<InputState> {
    let (ph, val) = (placeholder.to_string(), value.to_string());
    let state = cx.new(|cx| {
        let mut s = InputState::new(window, cx).placeholder(ph).default_value(val);
        if masked {
            s = s.masked(true);
        }
        s
    });
    ui::notify_on_focus(&state, window, cx);
    state
}
