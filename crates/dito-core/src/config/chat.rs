use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What the chat tool remembers between launches.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatSettings {
    /// The thinking choice per text model: `off`, `on` or the key of a level. The key names the model: `local:` with
    /// the catalog id, or `cloud:` with the configuration id.
    pub thinking: BTreeMap<String, String>,
}
