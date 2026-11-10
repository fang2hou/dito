use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The templates that ship with the app, in tab order: id, display name and JSON config. An id is saved in the
/// settings and names the profile's Keychain account, so it never changes once shipped; a new template is a new row.
const BUNDLED: &[(&str, &str, &str)] = &[
    ("funasr", "FunASR", include_str!("../../../../design/asr-templates/funasr.json")),
    ("qwen_asr", "Qwen ASR", include_str!("../../../../design/asr-templates/qwen-asr.json")),
    ("openrouter", "OpenRouter", include_str!("../../../../design/asr-templates/openrouter.json")),
];

/// One cloud speech connection the user can pick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AsrProfile {
    /// Stable identity; also the suffix of the profile's Keychain account.
    pub id: String,
    pub name: String,
    #[serde(with = "config_json")]
    pub config: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AsrProfiles {
    /// Id of the active profile.
    pub selected: String,
    pub profiles: Vec<AsrProfile>,
}

impl Default for AsrProfiles {
    fn default() -> Self {
        let profiles: Vec<AsrProfile> = BUNDLED
            .iter()
            .map(|&(id, name, template)| AsrProfile {
                id: id.into(),
                name: name.into(),
                config: serde_json::from_str(template).expect("bundled ASR template"),
            })
            .collect();
        Self { selected: profiles[0].id.clone(), profiles }
    }
}

impl AsrProfiles {
    fn selected_profile(&self) -> Option<&AsrProfile> {
        self.profiles.iter().find(|profile| profile.id == self.selected)
    }

    /// Config of the selected profile, `Null` when no profile has the selected id; validation rejects that before a
    /// request is made.
    pub fn active(&self) -> &Value {
        self.selected_profile().map_or(&Value::Null, |profile| &profile.config)
    }

    pub fn active_mut(&mut self) -> Option<&mut Value> {
        let selected = &self.selected;
        self.profiles.iter_mut().find(|profile| profile.id == *selected).map(|profile| &mut profile.config)
    }

    pub fn active_name(&self) -> Option<&str> {
        self.selected_profile().map(|profile| profile.name.as_str())
    }

    /// Keychain account of the selected profile's key: its id behind a fixed prefix, which for the bundled profiles is
    /// the account name they had while profiles were identified by position.
    pub fn account(&self) -> String {
        format!("asr.{}", self.selected)
    }

    /// Position of the selected profile in `profiles` for the positional settings tabs; the first when no profile has
    /// the selected id.
    pub fn selected_index(&self) -> usize {
        self.profiles.iter().position(|profile| profile.id == self.selected).unwrap_or(0)
    }

    /// Select `profiles[index]`; an index past the list changes nothing.
    pub fn select_index(&mut self, index: usize) {
        if let Some(profile) = self.profiles.get(index) {
            self.selected = profile.id.clone();
        }
    }
}

/// A config is saved as a JSON string, not a TOML table: TOML has no `null`, so a table would lose the `auto` entry of
/// the language map.
mod config_json {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    pub fn serialize<S: serde::Serializer>(value: &Value, serializer: S) -> Result<S::Ok, S::Error> {
        value.to_string().serialize(serializer)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Value, D::Error> {
        let text = String::deserialize(deserializer)?;
        serde_json::from_str(&text).map_err(serde::de::Error::custom)
    }
}
