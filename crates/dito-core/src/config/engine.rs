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

    /// The engine's identity in history, see [`EngineLabels`]. A provider source with a configured cloud speech
    /// profile is labelled by that profile, the same condition `EngineFactory::transcriber_for` uses to run it.
    pub fn label(&self) -> String {
        match (self.source, &self.cloud_asr) {
            (Source::Local, _) => format!("local:{}", self.local_model),
            (Source::Provider, Some(profiles)) => {
                let model = profiles.active()["connection"]["model"].as_str().unwrap_or("");
                format!("{}:{model}", profiles.account())
            }
            (Source::Provider, None) => {
                let provider = &self.provider;
                let model = match provider.kind {
                    ProviderKind::Azure => &provider.azure.deployment,
                    ProviderKind::Bedrock => &provider.bedrock.model_id,
                    ProviderKind::Compat => &provider.compat.model,
                };
                format!("{}:{model}", provider.kind.id())
            }
        }
    }
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self::with_local(DEFAULT_ASR_LOCAL)
    }
}

/// Which engines produced a history entry: `local:<model id>` for an on-device model, `<provider id>:<model>` for a
/// provider (`azure:<deployment>`, `bedrock:<model id>`, `compat:<model>`), and `<keychain account>:<model>` for a
/// configured cloud speech profile (for example `asr.funasr:paraformer-v2`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EngineLabels {
    /// Speech recognition. `None` for an entry from before labels were recorded, or one that never reached recognition.
    pub asr: Option<String>,
    /// Polishing. `None` when polishing was off, was never reached, or the entry predates labels.
    pub polish: Option<String>,
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
