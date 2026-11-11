use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{CloudAsrConfigurations, CloudPolishConfiguration, CloudPolishConfigurations};
use crate::i18n::Lang;

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

/// A field that is not set uses the value the catalog recommends for the model.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LocalPolishParams {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    pub cloud_asr: Option<CloudAsrConfigurations>,
    /// Polish only. `None` until a list is first saved; the templates stand in.
    pub cloud_polish: Option<CloudPolishConfigurations>,
    pub source: Source,
    pub local_model: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub local_params: BTreeMap<String, LocalPolishParams>,
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

    /// `lang` is the UI language, not necessarily the system's; it names the templates' three while none are saved.
    pub fn polish_configurations(&self, lang: Lang) -> Cow<'_, CloudPolishConfigurations> {
        self.cloud_polish.as_ref().map_or_else(|| Cow::Owned(CloudPolishConfigurations::new(lang)), Cow::Borrowed)
    }

    /// The first edit saves the templates' three, named in `lang`.
    pub fn polish_configurations_mut(&mut self, lang: Lang) -> &mut CloudPolishConfigurations {
        self.cloud_polish.get_or_insert_with(|| CloudPolishConfigurations::new(lang))
    }

    /// For the code that sends requests: it reads no name, so it needs no language.
    pub fn active_polish(&self) -> Option<Cow<'_, CloudPolishConfiguration>> {
        match &self.cloud_polish {
            Some(saved) => saved.active().map(Cow::Borrowed),
            None => Some(Cow::Owned(CloudPolishConfigurations::stand_in_active())),
        }
    }

    pub fn local_params_for(&self, model: &str) -> LocalPolishParams {
        self.local_params.get(model).copied().unwrap_or_default()
    }

    /// Save the parameters of the local model `model`. Parameters that set nothing are not kept.
    pub fn set_local_params(&mut self, model: &str, params: LocalPolishParams) {
        if params == LocalPolishParams::default() {
            self.local_params.remove(model);
        } else {
            self.local_params.insert(model.to_owned(), params);
        }
    }

    /// A cloud source with a configured speech list is labelled by that configuration, the condition
    /// `EngineFactory::transcriber_for` uses to run it.
    pub fn label(&self) -> String {
        match (self.source, &self.cloud_asr) {
            (Source::Local, _) => format!("local:{}", self.local_model),
            (Source::Provider, Some(configurations)) => configurations
                .active()
                .map(|active| {
                    format!("{}:{}", active.account(), active.config["connection"]["model"].as_str().unwrap_or(""))
                })
                .unwrap_or_default(),
            (Source::Provider, None) => self.active_polish().map(|active| active.label()).unwrap_or_default(),
        }
    }
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self::with_local(DEFAULT_ASR_LOCAL)
    }
}

/// Which engines produced a history entry: `local:<model id>` for an on-device model, `<template>:<model>` for a cloud
/// polish configuration (`azure:<deployment>`, `bedrock:<model id>`, `compat:<model>`), and
/// `<keychain account>:<model>` for a configured cloud speech configuration (for example `asr.funasr:paraformer-v2`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EngineLabels {
    /// `None` for an entry from before labels were recorded, or one that never reached transcription.
    pub asr: Option<String>,
    /// `None` when polishing was off, was never reached, or the entry predates labels.
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

#[cfg(test)]
mod tests {
    use super::*;

    const COMPAT_NAMES: [(Lang, &str); 3] =
        [(Lang::Ja, "OpenAI 互換 API"), (Lang::En, "OpenAI-compatible API"), (Lang::Zh, "OpenAI 兼容 API")];

    #[test]
    fn stand_in_configurations_are_named_in_the_language_they_are_asked_for() {
        let engine = EngineConfig::default();
        assert!(engine.cloud_polish.is_none(), "nothing is saved, so these are the stand-ins");
        for (lang, compat) in COMPAT_NAMES {
            assert_eq!(engine.polish_configurations(lang).get("compat").unwrap().name, compat);
        }
    }

    #[test]
    fn the_first_edit_saves_the_templates_in_the_language_the_ui_is_shown_in() {
        for (lang, compat) in COMPAT_NAMES {
            let mut engine = EngineConfig::default();
            engine.polish_configurations_mut(lang).select("azure");
            // What was saved stays as saved when the list is read in another language.
            let other = if lang == Lang::Ja { Lang::En } else { Lang::Ja };
            assert_eq!(engine.polish_configurations(other).get("compat").unwrap().name, compat);
        }
    }

    #[test]
    fn requests_run_the_selected_configuration_or_the_first_template_while_none_is_saved() {
        let mut engine = EngineConfig { source: Source::Provider, ..EngineConfig::default() };
        assert_eq!(engine.active_polish().unwrap().id, "azure");
        assert_eq!(engine.label(), "azure:", "history labels do not depend on any name");

        engine.polish_configurations_mut(Lang::En).select("compat");
        assert_eq!(engine.active_polish().unwrap().id, "compat");
        engine.polish_configurations_mut(Lang::En).selected = "gone".into();
        assert!(engine.active_polish().is_none(), "a selection that points nowhere is not replaced by a template");
    }

    #[test]
    fn a_speech_label_names_the_selected_configuration_and_nothing_when_the_selection_points_nowhere() {
        let mut engine = EngineConfig {
            source: Source::Provider,
            cloud_asr: Some(CloudAsrConfigurations::default()),
            ..EngineConfig::default()
        };
        engine.cloud_asr.as_mut().unwrap().active_mut().unwrap()["connection"]["model"] = "paraformer-v2".into();
        assert_eq!(engine.label(), "asr.funasr:paraformer-v2");
        engine.cloud_asr.as_mut().unwrap().selected = "gone".into();
        assert_eq!(engine.label(), "", "no configuration is substituted for the missing one");
    }
}
