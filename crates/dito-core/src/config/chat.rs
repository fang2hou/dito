use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::Source;

/// When a separate local chat model is in memory (`design:settings/local-models/chat-model`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatLoading {
    /// Loaded with the other models in use, so a chat never waits for it.
    #[default]
    Always,
    /// Loaded when the chat opens and released a while after it is left.
    WhileOpen,
}

/// What the chat tool remembers between launches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatSettings {
    /// The thinking choice per text model: `off`, `on` or the key of a level. The key names the model: `local:` with
    /// the catalog id, or `cloud:` with the configuration id.
    pub thinking: BTreeMap<String, String>,
    /// The chat uses the text model; the fields below only apply when it does not.
    pub same_as_text: bool,
    pub source: Source,
    /// A catalog id of a local text model.
    pub local_model: String,
    /// The id of one of the text model's cloud configurations.
    pub cloud_text: String,
    pub loading: ChatLoading,
}

impl Default for ChatSettings {
    fn default() -> Self {
        Self {
            thinking: BTreeMap::new(),
            same_as_text: true,
            source: Source::Local,
            local_model: String::new(),
            cloud_text: String::new(),
            loading: ChatLoading::Always,
        }
    }
}
