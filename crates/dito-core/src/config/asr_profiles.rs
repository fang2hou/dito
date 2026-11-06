use serde::{Deserialize, Serialize};
use serde_json::Value;

const ACCOUNTS: [&str; 3] = ["funasr", "qwen_asr", "openrouter"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AsrProfiles {
    pub selected: usize,
    #[serde(with = "profile_json")]
    pub profiles: Vec<Value>,
}

impl Default for AsrProfiles {
    fn default() -> Self {
        Self {
            selected: 0,
            profiles: [
                include_str!("../../../../design/asr-templates/funasr.json"),
                include_str!("../../../../design/asr-templates/qwen-asr.json"),
                include_str!("../../../../design/asr-templates/openrouter.json"),
            ]
            .iter()
            .map(|s| serde_json::from_str(s).expect("bundled ASR template"))
            .collect(),
        }
    }
}

impl AsrProfiles {
    pub const NAMES: [&'static str; 3] = ["FunASR", "Qwen ASR", "OpenRouter"];

    pub fn active(&self) -> &Value {
        self.profiles.get(self.selected).unwrap_or(&Value::Null)
    }

    /// Keychain account of the active profile's key. An out-of-range selection falls back to the first
    /// profile's account; `active` is `Null` then, so validation rejects it before a request is made.
    pub fn account(&self) -> String {
        format!("asr.{}", ACCOUNTS.get(self.selected).unwrap_or(&ACCOUNTS[0]))
    }
}

mod profile_json {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    pub fn serialize<S: serde::Serializer>(value: &[Value], serializer: S) -> Result<S::Ok, S::Error> {
        value.iter().map(Value::to_string).collect::<Vec<_>>().serialize(serializer)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<Value>, D::Error> {
        Vec::<String>::deserialize(deserializer)?
            .into_iter()
            .map(|s| serde_json::from_str(&s).map_err(serde::de::Error::custom))
            .collect()
    }
}
