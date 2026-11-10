use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{AsrProfiles, PolishConfiguration, PolishConfigurations};

pub const DEFAULT_ASR_LOCAL: &str = "qwen3-asr-1.7b";
pub const DEFAULT_POLISH_LOCAL: &str = "qwen3.5-9b-instruct";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    Local,
    Provider,
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

/// How a local polish model samples its reply. A field that is not set uses the value the catalog recommends for that
/// model.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LocalParams {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    pub cloud_asr: Option<AsrProfiles>,
    /// Polish only. `None` until a list is first saved; the three templates stand in, as the bundled profiles do for
    /// speech.
    pub cloud_polish: Option<PolishConfigurations>,
    pub source: Source,
    pub local_model: String,
    /// Polish only: the sampling of each local model that has any set, by model id.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub local_params: BTreeMap<String, LocalParams>,
}

impl EngineConfig {
    fn with_local(model: &str) -> Self {
        Self {
            source: Source::Local,
            local_model: model.into(),
            cloud_asr: None,
            cloud_polish: None,
            local_params: BTreeMap::new(),
        }
    }

    /// The saved polish configurations, or the templates' three while none are saved.
    pub fn polish_configurations(&self) -> Cow<'_, PolishConfigurations> {
        self.cloud_polish.as_ref().map_or_else(|| Cow::Owned(PolishConfigurations::default()), Cow::Borrowed)
    }

    /// The saved polish configurations for editing; the first edit saves the templates' three.
    pub fn polish_configurations_mut(&mut self) -> &mut PolishConfigurations {
        self.cloud_polish.get_or_insert_with(PolishConfigurations::default)
    }

    /// The parameters set for the local model `model`; all unset when there are none.
    pub fn local_params(&self, model: &str) -> LocalParams {
        self.local_params.get(model).copied().unwrap_or_default()
    }

    /// Save the parameters of the local model `model`. Parameters that set nothing are not kept.
    pub fn set_local_params(&mut self, model: &str, params: LocalParams) {
        if params == LocalParams::default() {
            self.local_params.remove(model);
        } else {
            self.local_params.insert(model.to_owned(), params);
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
                self.polish_configurations().active().map(PolishConfiguration::label).unwrap_or_default()
            }
        }
    }
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self::with_local(DEFAULT_ASR_LOCAL)
    }
}

/// Which engines produced a history entry: `local:<model id>` for an on-device model, `<template>:<model>` for a cloud
/// polish configuration (`azure:<deployment>`, `bedrock:<model id>`, `compat:<model>`), and `<keychain account>:<model>`
/// for a configured cloud speech profile (for example `asr.funasr:paraformer-v2`).
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
