use std::borrow::Cow;
use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{
    ChatLoading, ChatSettings, CloudAsrConfigurations, DEFAULT_TEXT_LOCAL, EngineConfig, General, LocalApiSettings,
    PolishConfig, ReasoningEffort, SCHEMA_VERSION, Source, TranscriptionSettings,
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
        if self.chat.loading == ChatLoading::Always
            && let Some(id) = self.separate_chat_model()
        {
            ids.insert(id.to_owned());
        }
        ids
    }

    /// The engine the chat tool talks to: the text model, or the chat's own choice. A local choice keeps the
    /// parameters saved for that model; a cloud choice is one of the text model's configurations, and one that no
    /// longer exists leaves the text model's selection in place.
    pub fn chat_engine(&self) -> Cow<'_, EngineConfig> {
        if self.chat.same_as_text {
            return Cow::Borrowed(&self.text_model);
        }
        let mut engine = self.text_model.clone();
        engine.source = self.chat.source;
        match self.chat.source {
            Source::Local if !self.chat.local_model.is_empty() => engine.local_model.clone_from(&self.chat.local_model),
            Source::Local => {}
            Source::Provider => {
                let mut configurations = engine.text_configurations(self.general.lang()).into_owned();
                configurations.select(&self.chat.cloud_text);
                engine.cloud_text = Some(configurations);
            }
        }
        Cow::Owned(engine)
    }

    /// A local chat model other than the text model: the only case where the chat holds a model of its own in memory.
    pub fn separate_chat_model(&self) -> Option<&str> {
        let chat = &self.chat;
        let shares_text_model =
            self.text_model.source == Source::Local && self.text_model.local_model == chat.local_model;
        (!chat.same_as_text && chat.source == Source::Local && !chat.local_model.is_empty() && !shares_text_model)
            .then_some(chat.local_model.as_str())
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

    #[test]
    fn the_chat_follows_the_text_model_until_it_picks_its_own() {
        let mut c = Config::default();
        assert_eq!(*c.chat_engine(), c.text_model);
        assert_eq!(c.separate_chat_model(), None);

        c.chat.same_as_text = false;
        c.chat.local_model = "qwen3.5-4b-instruct".into();
        assert_eq!(c.chat_engine().local_model, "qwen3.5-4b-instruct");
        assert_eq!(c.separate_chat_model(), Some("qwen3.5-4b-instruct"));
        assert!(c.local_models_in_use().contains("qwen3.5-4b-instruct"), "kept loaded by default");

        c.chat.loading = ChatLoading::WhileOpen;
        assert!(!c.local_models_in_use().contains("qwen3.5-4b-instruct"), "loaded only while the chat is open");

        c.chat.local_model.clone_from(&c.text_model.local_model);
        assert_eq!(c.separate_chat_model(), None, "the text model is already the one in memory");
    }

    #[test]
    fn a_cloud_chat_picks_one_of_the_text_configurations_and_falls_back_to_the_text_selection() {
        let mut c = Config::default();
        c.chat.same_as_text = false;
        c.chat.source = Source::Provider;
        c.chat.cloud_text = "compat".into();
        assert_eq!(c.chat_engine().active_text().unwrap().id, "compat");
        assert_eq!(c.separate_chat_model(), None);

        c.chat.cloud_text = "gone".into();
        assert_eq!(c.chat_engine().active_text().unwrap().id, "azure");
    }
}
