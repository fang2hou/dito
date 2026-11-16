use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{
    ChatSettings, CloudAsrConfigurations, DEFAULT_TEXT_LOCAL, EngineConfig, General, LocalApiSettings, PolishConfig,
    ReasoningEffort, SCHEMA_VERSION, Source, TranscriptionSettings,
};
use crate::prompts::Prompts;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Version of the persisted shape (see `migrate`). Declared first so it is the first line of the saved file.
    pub schema_version: u32,
    pub general: General,
    pub asr: EngineConfig,
    pub polish: PolishConfig,
    pub text_model: EngineConfig,
    pub prompts: Prompts,
    pub wordbook_for_polish: bool,
    pub onboarded: bool,
    pub models_dir: Option<PathBuf>,
    pub transcription: TranscriptionSettings,
    pub local_api: LocalApiSettings,
    pub chat: ChatSettings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            general: General::default(),
            asr: EngineConfig::default(),
            polish: PolishConfig::default(),
            text_model: EngineConfig::with_local(DEFAULT_TEXT_LOCAL),
            prompts: Prompts::default(),
            wordbook_for_polish: false,
            onboarded: false,
            models_dir: None,
            transcription: TranscriptionSettings::default(),
            local_api: LocalApiSettings::default(),
            chat: ChatSettings::default(),
        }
    }
}

impl Config {
    /// No endpoint is set; the caller points the active speech configuration and the Azure polish endpoint at a
    /// server.
    pub fn dev_azure() -> Self {
        let mut c = Self::default();
        c.asr.source = Source::Provider;
        c.asr.cloud_asr = Some(CloudAsrConfigurations::default());
        c.text_model.source = Source::Provider;
        let configurations = c.text_model.text_configurations_mut(c.general.lang());
        configurations.select("azure");
        let azure = configurations.active_mut().expect("the templates include an azure configuration");
        *azure.target.model_mut() = "gpt-6.1-terra".into();
        azure.params.reasoning_effort = ReasoningEffort::Low;
        c
    }

    pub fn local_models_in_use(&self) -> BTreeSet<String> {
        let mut ids = BTreeSet::new();
        if self.asr.source == Source::Local {
            ids.insert(self.asr.local_model.clone());
        }
        if self.polish.enabled && self.text_model.source == Source::Local {
            ids.insert(self.text_model.local_model.clone());
        }
        ids
    }

    pub fn engine_labels(&self) -> super::EngineLabels {
        let polish = self.polish.enabled.then(|| self.text_model.label());
        super::EngineLabels { asr: Some(self.asr.label()), polish }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_local_enabled_engines_count_as_models_in_use() {
        let mut c = Config::default();
        assert_eq!(c.local_models_in_use().len(), 2, "default is local ASR + local polish");
        c.polish.enabled = false;
        assert_eq!(c.local_models_in_use(), BTreeSet::from([c.asr.local_model.clone()]));
        c.asr.source = Source::Provider;
        assert!(c.local_models_in_use().is_empty());
        c.polish.enabled = true;
        assert_eq!(c.local_models_in_use(), BTreeSet::from([c.text_model.local_model.clone()]));
    }
}
