//! A JSON value saved as a string, for `#[serde(with)]`: TOML has no `null`, so a table would lose every `null` in it.
//! What is accepted back is whatever the field's type accepts.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn serialize<T: Serialize, S: Serializer>(value: &T, serializer: S) -> Result<S::Ok, S::Error> {
    serde_json::to_string(value).map_err(serde::ser::Error::custom)?.serialize(serializer)
}

pub fn deserialize<'de, T: DeserializeOwned, D: Deserializer<'de>>(deserializer: D) -> Result<T, D::Error> {
    let text = String::deserialize(deserializer)?;
    serde_json::from_str(&text).map_err(serde::de::Error::custom)
}
