use serde::{Deserialize, Serialize};

use crate::i18n::Tr;

use super::AsrProfiles;

pub const DEFAULT_ASR_LOCAL: &str = "qwen3-asr-1.7b";
pub const DEFAULT_POLISH_LOCAL: &str = "qwen3.5-9b-instruct";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    Local,
    Provider,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[default]
    Azure,
    Bedrock,
    Compat,
}

impl ProviderKind {
    pub const ALL: [ProviderKind; 3] = [Self::Azure, Self::Bedrock, Self::Compat];

    pub fn label(self) -> Tr {
        match self {
            Self::Azure => Tr::ModelsProviderAzure,
            Self::Bedrock => Tr::ModelsProviderBedrock,
            Self::Compat => Tr::ModelsOpenaiCompatible,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Azure => "azure",
            Self::Bedrock => "bedrock",
            Self::Compat => "compat",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(from = "String", into = "String")]
pub enum ReasoningEffort {
    Custom(String),
    Minimal,
    #[default]
    Low,
    Medium,
    High,
}

impl ReasoningEffort {
    pub const ALL: [ReasoningEffort; 4] = [Self::Minimal, Self::Low, Self::Medium, Self::High];

    pub fn as_str(&self) -> &str {
        match self {
            Self::Custom(value) => value,
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

impl From<String> for ReasoningEffort {
    fn from(value: String) -> Self {
        match value.as_str() {
            "minimal" => Self::Minimal,
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            _ => Self::Custom(value),
        }
    }
}
impl From<ReasoningEffort> for String {
    fn from(value: ReasoningEffort) -> Self {
        value.as_str().to_owned()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AzureSettings {
    pub endpoint: String,
    pub deployment: String,
    pub reasoning_effort: ReasoningEffort,
}

impl Default for AzureSettings {
    fn default() -> Self {
        Self { endpoint: String::new(), deployment: String::new(), reasoning_effort: ReasoningEffort::Low }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BedrockSettings {
    pub profile: String,
    pub region: String,
    pub model_id: String,
    pub reasoning_effort: ReasoningEffort,
}

impl Default for BedrockSettings {
    fn default() -> Self {
        Self {
            profile: String::new(),
            region: "ap-northeast-1".into(),
            model_id: String::new(),
            reasoning_effort: ReasoningEffort::Custom(String::new()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompatSettings {
    pub base_url: String,
    pub model: String,
    pub reasoning_effort: ReasoningEffort,
}

impl Default for CompatSettings {
    fn default() -> Self {
        Self { base_url: String::new(), model: String::new(), reasoning_effort: ReasoningEffort::Low }
    }
}

/// All three provider forms are kept so switching the tab never loses what was typed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProviderSettings {
    pub kind: ProviderKind,
    pub azure: AzureSettings,
    pub bedrock: BedrockSettings,
    pub compat: CompatSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    pub cloud_asr: Option<AsrProfiles>,
    pub source: Source,
    pub local_model: String,
    pub provider: ProviderSettings,
}

impl EngineConfig {
    fn with_local(model: &str) -> Self {
        Self {
            source: Source::Local,
            local_model: model.into(),
            provider: ProviderSettings::default(),
            cloud_asr: None,
        }
    }
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self::with_local(DEFAULT_ASR_LOCAL)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PolishConfig {
    pub enabled: bool,
    #[serde(flatten)]
    pub engine: EngineConfig,
}

impl Default for PolishConfig {
    fn default() -> Self {
        Self { enabled: true, engine: EngineConfig::with_local(DEFAULT_POLISH_LOCAL) }
    }
}
