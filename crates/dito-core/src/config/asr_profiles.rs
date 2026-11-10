use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::list::{Entry, List};
use crate::secrets::asr_account;

/// A bundled request JSON a cloud speech profile can start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsrTemplate {
    /// Saved in the settings as the id of the profile made first from it, and part of that profile's Keychain account,
    /// so it never changes once shipped; a new template is a new row.
    pub id: &'static str,
    pub name: &'static str,
    config: &'static str,
}

impl AsrTemplate {
    /// The templates that ship with the app, in tab order.
    pub const ALL: [AsrTemplate; 3] = [
        AsrTemplate {
            id: "funasr",
            name: "FunASR",
            config: include_str!("../../../../design/asr-templates/funasr.json"),
        },
        AsrTemplate {
            id: "qwen_asr",
            name: "Qwen ASR",
            config: include_str!("../../../../design/asr-templates/qwen-asr.json"),
        },
        AsrTemplate {
            id: "openrouter",
            name: "OpenRouter",
            config: include_str!("../../../../design/asr-templates/openrouter.json"),
        },
    ];

    /// The request JSON of the template.
    pub fn config(self) -> Value {
        serde_json::from_str(self.config).expect("bundled ASR template")
    }

    fn profile(self, id: &str, name: &str) -> AsrProfile {
        AsrProfile { id: id.into(), name: name.into(), config: self.config() }
    }
}

/// One cloud speech connection the user can pick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AsrProfile {
    /// Stable identity; also the suffix of the profile's Keychain account.
    pub id: String,
    pub name: String,
    #[serde(with = "config_json")]
    pub config: Value,
}

impl AsrProfile {
    /// Keychain account of this profile's API key.
    pub fn account(&self) -> String {
        asr_account(&self.id)
    }
}

impl Entry for AsrProfile {
    fn id(&self) -> &str {
        &self.id
    }

    fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }
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
        let profiles: Vec<AsrProfile> =
            AsrTemplate::ALL.iter().map(|template| template.profile(template.id, template.name)).collect();
        Self { selected: profiles[0].id.clone(), profiles }
    }
}

impl AsrProfiles {
    fn selected_profile(&self) -> Option<&AsrProfile> {
        self.profiles.iter().find(|profile| profile.id == self.selected)
    }

    fn list(&mut self) -> List<'_, AsrProfile> {
        List::new(&mut self.selected, &mut self.profiles)
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
        asr_account(&self.selected)
    }

    /// Id of the selected profile, or of the first one when no profile has the selected id.
    pub fn selected_id(&self) -> &str {
        self.selected_profile().or(self.profiles.first()).map_or("", |profile| profile.id.as_str())
    }

    /// Make `id` the profile in use. An unknown id changes nothing.
    pub fn select(&mut self, id: &str) -> bool {
        self.list().select(id)
    }

    /// Append a profile that starts from `template`, select it and return its id.
    pub fn add_from_template(&mut self, template: AsrTemplate, name: &str) -> String {
        let id = self.list().fresh_id(template.id);
        self.list().push(template.profile(&id, name));
        id
    }

    /// Append a copy of `id` under a new id, select it and return that id. The key is not copied: it belongs to the
    /// source's Keychain account.
    pub fn duplicate(&mut self, id: &str, name: &str) -> Option<String> {
        let mut copy = self.profiles.iter().find(|profile| profile.id == id)?.clone();
        // A profile made earlier carries a random suffix after the id of its template: a copy starts from the template.
        let base = copy.id.split('-').next().unwrap_or(&copy.id);
        copy.id = self.list().fresh_id(base);
        copy.name = name.to_owned();
        let id = copy.id.clone();
        self.list().push(copy);
        Some(id)
    }

    /// Take a profile out of the list and return it. Its key is still under its `account()`:
    /// `ConfigStore::remove_asr_profile` removes both, deleting the key only once the list is saved. The last profile
    /// stays, so this returns `None` for it. When the profile in use goes, its neighbour is selected.
    pub fn remove(&mut self, id: &str) -> Option<AsrProfile> {
        self.list().remove(id)
    }

    /// Put `id` at `index` of the list as it is afterwards.
    pub fn move_to(&mut self, id: &str, index: usize) -> bool {
        self.list().move_to(id, index)
    }

    /// Rename `id`; a blank name is refused.
    pub fn rename(&mut self, id: &str, name: &str) -> bool {
        self.list().rename(id, name)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::list::contract::{Editable, check_editing};

    impl Editable for AsrProfiles {
        fn ids(&self) -> Vec<String> {
            self.profiles.iter().map(|profile| profile.id.clone()).collect()
        }
        fn selected(&self) -> String {
            self.selected.clone()
        }
        fn name_of(&self, id: &str) -> Option<String> {
            self.profiles.iter().find(|profile| profile.id == id).map(|profile| profile.name.clone())
        }
        fn select(&mut self, id: &str) -> bool {
            AsrProfiles::select(self, id)
        }
        fn add(&mut self, name: &str) -> String {
            self.add_from_template(AsrTemplate::ALL[0], name)
        }
        fn duplicate(&mut self, id: &str, name: &str) -> Option<String> {
            AsrProfiles::duplicate(self, id, name)
        }
        fn remove(&mut self, id: &str) -> bool {
            AsrProfiles::remove(self, id).is_some()
        }
        fn move_to(&mut self, id: &str, index: usize) -> bool {
            AsrProfiles::move_to(self, id, index)
        }
        fn rename(&mut self, id: &str, name: &str) -> bool {
            AsrProfiles::rename(self, id, name)
        }
    }

    #[test]
    fn the_editing_api_behaves_as_documented() {
        check_editing(&mut AsrProfiles::default());
    }

    #[test]
    fn the_defaults_are_the_bundled_templates_with_their_ids_as_accounts() {
        let profiles = AsrProfiles::default();
        let identities: Vec<_> = profiles.profiles.iter().map(|p| (p.id.as_str(), p.name.as_str())).collect();
        assert_eq!(identities, [("funasr", "FunASR"), ("qwen_asr", "Qwen ASR"), ("openrouter", "OpenRouter")]);
        assert_eq!(profiles.selected, "funasr");
        assert_eq!(profiles.account(), "asr.funasr");
        assert_eq!(profiles.profiles[2].account(), "asr.openrouter");
    }

    #[test]
    fn a_new_profile_starts_from_its_template_and_a_duplicate_copies_the_edited_config() {
        let mut profiles = AsrProfiles::default();
        let template = AsrTemplate::ALL[1];
        let added = profiles.add_from_template(template, "Mine");
        assert!(added.starts_with("qwen_asr-"), "{added}");
        assert_eq!(profiles.active(), &template.config());

        profiles.active_mut().unwrap()["connection"]["model"] = "edited".into();
        let copy = profiles.duplicate(&added, "Mine too").unwrap();
        assert!(copy.starts_with("qwen_asr-") && copy != added, "a copy of a copy starts from the template id: {copy}");
        assert_eq!(profiles.active()["connection"]["model"], "edited");
        assert_ne!(profiles.account(), format!("asr.{added}"), "the copy has a key of its own");
    }
}
