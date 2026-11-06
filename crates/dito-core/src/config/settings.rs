use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{EngineConfig, General, PolishConfig, ProviderKind, ReasoningEffort, Source};
use crate::prompts::Prompts;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub asr: EngineConfig,
    pub polish: PolishConfig,
    pub prompts: Prompts,
    /// Also pass dictionary words to the polish model.
    pub dictionary_for_polish: bool,
    pub onboarded: bool,
    pub models_dir: Option<PathBuf>,
}

impl Config {
    /// First-run defaults for development: Azure MAI-Transcribe-2 + gpt-6.1-terra (low).
    pub fn dev_azure() -> Self {
        let mut c = Self::default();
        c.asr.source = Source::Provider;
        c.asr.provider.kind = ProviderKind::Azure;
        c.asr.provider.azure.deployment = "MAI-Transcribe-2".into();
        c.polish.engine.source = Source::Provider;
        c.polish.engine.provider.kind = ProviderKind::Azure;
        c.polish.engine.provider.azure.deployment = "gpt-6.1-terra".into();
        c.polish.engine.provider.azure.reasoning_effort = ReasoningEffort::Low;
        c
    }

    /// Ids of the local models the current settings would run: speech recognition when it is local, and
    /// polishing when it is enabled and local.
    pub fn local_models_in_use(&self) -> BTreeSet<String> {
        let mut ids = BTreeSet::new();
        if self.asr.source == Source::Local {
            ids.insert(self.asr.local_model.clone());
        }
        if self.polish.enabled && self.polish.engine.source == Source::Local {
            ids.insert(self.polish.engine.local_model.clone());
        }
        ids
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
        assert_eq!(c.local_models_in_use(), BTreeSet::from([c.polish.engine.local_model.clone()]));
    }
}
