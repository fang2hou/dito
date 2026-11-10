use std::ops::Index;

use dito_core::config::{PolishConfiguration, PolishTemplate};
use dito_core::i18n::{Lang, Tr};
use dito_core::secrets::Role;
use gpui_kit::*;

use super::page::ModelsPage;
use crate::settings::role;
use crate::state::AppState;
use crate::ui::{self, InputState};

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

/// Which part of a configuration a field edits.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Address,
    Model,
    Effort,
    Key,
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

    /// The template whose form has this field.
    fn template(self) -> PolishTemplate {
        match self {
            Self::AzureEndpoint | Self::AzureDeployment | Self::EffortAzure | Self::KeyAzure => PolishTemplate::Azure,
            Self::BedrockRegion | Self::BedrockModel | Self::EffortBedrock => PolishTemplate::Bedrock,
            Self::CompatBase | Self::CompatModel | Self::EffortCompat | Self::KeyCompat => PolishTemplate::Compat,
        }
    }

    fn part(self) -> Part {
        match self {
            Self::AzureEndpoint | Self::BedrockRegion | Self::CompatBase => Part::Address,
            Self::AzureDeployment | Self::BedrockModel | Self::CompatModel => Part::Model,
            Self::EffortAzure | Self::EffortBedrock | Self::EffortCompat => Part::Effort,
            Self::KeyAzure | Self::KeyCompat => Part::Key,
        }
    }

    /// Key fields live in the secret store, under the account of the configuration in use.
    pub(super) fn is_key(self) -> bool {
        self.part() == Part::Key
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

    /// What the field shows for `config`: empty for a configuration of another template, whose form this is not.
    fn value(self, config: &PolishConfiguration, st: &AppState) -> String {
        if self.template() != config.target.template() {
            return String::new();
        }
        match self.part() {
            Part::Address => config.target.address().to_owned(),
            Part::Model => config.target.model().to_owned(),
            Part::Effort => config.params.reasoning_effort.as_str().to_owned(),
            Part::Key => st.secrets.get(&config.account()).unwrap_or_default(),
        }
    }

    /// Store an edited value into the configuration; key fields live in the secret store instead.
    pub(super) fn write(self, config: &mut PolishConfiguration, value: String) {
        if self.template() != config.target.template() {
            return;
        }
        match self.part() {
            Part::Address => *config.target.address_mut() = value,
            Part::Model => *config.target.model_mut() = value,
            Part::Effort => config.params.reasoning_effort = value.into(),
            Part::Key => {}
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|f| *f == self).expect("every field is listed in ALL")
    }
}

/// One text input per field of the polish forms; speech is edited in the cloud ASR editor instead. They show the
/// configuration in use.
pub(super) struct ProviderInputs([Entity<InputState>; 11]);

impl ProviderInputs {
    pub(super) fn new(st: &AppState, window: &mut Window, cx: &mut Context<ModelsPage>) -> Self {
        let cfg = st.cfg.get();
        let engine = role::engine(&cfg, Role::Polish);
        let configurations = engine.polish_configurations();
        let active = configurations.active();
        let lang = cfg.general.lang();
        Self(Field::ALL.map(|field| {
            let value = active.map(|config| field.value(config, st)).unwrap_or_default();
            make_input(window, cx, field.placeholder(lang), &value, field.is_key())
        }))
    }

    /// Show `config` in every input, after another configuration was selected. Setting a value is not an edit.
    pub(super) fn show(
        &self,
        config: Option<&PolishConfiguration>,
        st: &AppState,
        window: &mut Window,
        cx: &mut Context<ModelsPage>,
    ) {
        for field in Field::ALL {
            let value = config.map(|config| field.value(config, st)).unwrap_or_default();
            self[field].update(cx, |input, cx| input.set_value(value, window, cx));
        }
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
