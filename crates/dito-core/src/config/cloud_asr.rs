use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::list::{Entry, List};
use crate::secrets::asr_account;

/// A bundled request JSON a cloud speech configuration can start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloudAsrTemplate {
    /// Saved in the settings as the id of the configuration made first from it, and part of that configuration's Keychain account,
    /// so it never changes once shipped; a new template is a new row.
    pub id: &'static str,
    pub name: &'static str,
    config: &'static str,
}

impl CloudAsrTemplate {
    /// The templates that ship with the app, in tab order.
    pub const ALL: [CloudAsrTemplate; 3] = [
        CloudAsrTemplate {
            id: "funasr",
            name: "FunASR",
            config: include_str!("../../../../design/asr-templates/funasr.json"),
        },
        CloudAsrTemplate {
            id: "qwen_asr",
            name: "Qwen ASR",
            config: include_str!("../../../../design/asr-templates/qwen-asr.json"),
        },
        CloudAsrTemplate {
            id: "openrouter",
            name: "OpenRouter",
            config: include_str!("../../../../design/asr-templates/openrouter.json"),
        },
    ];

    /// The request JSON of the template.
    pub fn config(self) -> Value {
        serde_json::from_str(self.config).expect("bundled ASR template")
    }

    fn configuration(self, id: &str, name: &str) -> CloudAsrConfiguration {
        CloudAsrConfiguration { id: id.into(), name: name.into(), config: self.config() }
    }
}

/// One saved cloud speech configuration the user can pick: a name, the request JSON and, outside it, a Keychain key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudAsrConfiguration {
    /// Stable identity; also the suffix of the configuration's Keychain account.
    pub id: String,
    pub name: String,
    #[serde(with = "config_json")]
    pub config: Value,
}

impl CloudAsrConfiguration {
    /// Keychain account of this configuration's API key.
    pub fn account(&self) -> String {
        asr_account(&self.id)
    }
}

impl Entry for CloudAsrConfiguration {
    fn id(&self) -> &str {
        &self.id
    }

    fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CloudAsrConfigurations {
    /// Id of the active configuration.
    pub selected: String,
    pub items: Vec<CloudAsrConfiguration>,
}

impl Default for CloudAsrConfigurations {
    fn default() -> Self {
        let items: Vec<CloudAsrConfiguration> =
            CloudAsrTemplate::ALL.iter().map(|template| template.configuration(template.id, template.name)).collect();
        Self { selected: items[0].id.clone(), items }
    }
}

impl CloudAsrConfigurations {
    fn list(&mut self) -> List<'_, CloudAsrConfiguration> {
        List::new(&mut self.selected, &mut self.items)
    }

    /// The configuration in use. `None` when no configuration has the selected id: nothing takes its place, so the
    /// id, the config and the Keychain account are all read from the same configuration or none.
    pub fn active(&self) -> Option<&CloudAsrConfiguration> {
        self.items.iter().find(|configuration| configuration.id == self.selected)
    }

    pub fn active_mut(&mut self) -> Option<&mut Value> {
        let selected = &self.selected;
        self.items
            .iter_mut()
            .find(|configuration| configuration.id == *selected)
            .map(|configuration| &mut configuration.config)
    }

    pub fn active_name(&self) -> Option<&str> {
        self.active().map(|configuration| configuration.name.as_str())
    }

    /// Make `id` the configuration in use. An unknown id changes nothing.
    pub fn select(&mut self, id: &str) -> bool {
        self.list().select(id)
    }

    /// Append a configuration that starts from `template`, select it and return its id.
    pub fn add_from_template(&mut self, template: CloudAsrTemplate, name: &str) -> String {
        let id = self.list().fresh_id(template.id);
        self.list().push(template.configuration(&id, name));
        id
    }

    /// Append a copy of `id` under a new id, select it and return that id. The key is not copied: it belongs to the
    /// source's Keychain account.
    pub fn duplicate(&mut self, id: &str, name: &str) -> Option<String> {
        let mut copy = self.items.iter().find(|configuration| configuration.id == id)?.clone();
        // A configuration made earlier carries a random suffix after the id of its template: a copy starts from the template.
        let base = copy.id.split('-').next().unwrap_or(&copy.id);
        copy.id = self.list().fresh_id(base);
        copy.name = name.to_owned();
        let id = copy.id.clone();
        self.list().push(copy);
        Some(id)
    }

    /// Take a configuration out of the list and return it. Its key is still under its `account()`:
    /// `ConfigStore::remove_cloud_asr_configuration` removes both, deleting the key only once the list is saved. The last configuration
    /// stays, so this returns `None` for it. When the configuration in use goes, its neighbour is selected.
    pub fn remove(&mut self, id: &str) -> Option<CloudAsrConfiguration> {
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

    impl Editable for CloudAsrConfigurations {
        fn ids(&self) -> Vec<String> {
            self.items.iter().map(|configuration| configuration.id.clone()).collect()
        }
        fn selected(&self) -> String {
            self.selected.clone()
        }
        fn name_of(&self, id: &str) -> Option<String> {
            self.items
                .iter()
                .find(|configuration| configuration.id == id)
                .map(|configuration| configuration.name.clone())
        }
        fn select(&mut self, id: &str) -> bool {
            CloudAsrConfigurations::select(self, id)
        }
        fn add(&mut self, name: &str) -> String {
            self.add_from_template(CloudAsrTemplate::ALL[0], name)
        }
        fn duplicate(&mut self, id: &str, name: &str) -> Option<String> {
            CloudAsrConfigurations::duplicate(self, id, name)
        }
        fn remove(&mut self, id: &str) -> bool {
            CloudAsrConfigurations::remove(self, id).is_some()
        }
        fn move_to(&mut self, id: &str, index: usize) -> bool {
            CloudAsrConfigurations::move_to(self, id, index)
        }
        fn rename(&mut self, id: &str, name: &str) -> bool {
            CloudAsrConfigurations::rename(self, id, name)
        }
    }

    #[test]
    fn the_editing_api_behaves_as_documented() {
        check_editing(&mut CloudAsrConfigurations::default());
    }

    #[test]
    fn the_defaults_are_the_bundled_templates_with_their_ids_as_accounts() {
        let configurations = CloudAsrConfigurations::default();
        let identities: Vec<_> = configurations.items.iter().map(|p| (p.id.as_str(), p.name.as_str())).collect();
        assert_eq!(identities, [("funasr", "FunASR"), ("qwen_asr", "Qwen ASR"), ("openrouter", "OpenRouter")]);
        assert_eq!(configurations.selected, "funasr");
        assert_eq!(configurations.active().unwrap().account(), "asr.funasr");
        assert_eq!(configurations.items[2].account(), "asr.openrouter");
    }

    #[test]
    fn a_selection_that_names_no_configuration_is_not_replaced_by_the_first() {
        let mut configurations = CloudAsrConfigurations { selected: "gone".into(), ..Default::default() };
        assert!(configurations.active().is_none());
        assert!(configurations.active_mut().is_none());
        assert_eq!(configurations.active_name(), None);

        configurations.selected = "openrouter".into();
        let active = configurations.active().unwrap();
        assert_eq!((active.id.as_str(), active.name.as_str()), ("openrouter", "OpenRouter"));
        assert_eq!(active.account(), "asr.openrouter");
    }

    #[test]
    fn a_new_configuration_starts_from_its_template_and_a_duplicate_copies_the_edited_config() {
        let mut configurations = CloudAsrConfigurations::default();
        let template = CloudAsrTemplate::ALL[1];
        let added = configurations.add_from_template(template, "Mine");
        assert!(added.starts_with("qwen_asr-"), "{added}");
        assert_eq!(configurations.active().unwrap().config, template.config());

        configurations.active_mut().unwrap()["connection"]["model"] = "edited".into();
        let copy = configurations.duplicate(&added, "Mine too").unwrap();
        assert!(copy.starts_with("qwen_asr-") && copy != added, "a copy of a copy starts from the template id: {copy}");
        let active = configurations.active().unwrap();
        assert_eq!(active.config["connection"]["model"], "edited");
        assert_ne!(active.account(), format!("asr.{added}"), "the copy has a key of its own");
    }
}
