use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::json_string;
use super::list::{Configurations, Entry};
use crate::secrets::asr_account;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloudAsrTemplate {
    /// Saved as the id of the first configuration made from it and part of its Keychain account, so it never changes
    /// once shipped.
    pub id: &'static str,
    pub name: &'static str,
    config: &'static str,
}

impl CloudAsrTemplate {
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

    pub fn config(self) -> Value {
        serde_json::from_str(self.config).expect("bundled ASR template")
    }

    fn configuration(self, id: &str, name: &str) -> CloudAsrConfiguration {
        CloudAsrConfiguration { id: id.into(), name: name.into(), config: self.config() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudAsrConfiguration {
    /// Stable; also the suffix of the Keychain account.
    pub id: String,
    pub name: String,
    /// Saved as a JSON string, not a TOML table: TOML has no `null`, so a table would lose the `auto` entry of the
    /// language map.
    #[serde(with = "json_string")]
    pub config: Value,
}

impl CloudAsrConfiguration {
    pub fn account(&self) -> String {
        asr_account(&self.id)
    }
}

impl Entry for CloudAsrConfiguration {
    type Template = CloudAsrTemplate;

    fn id(&self) -> &str {
        &self.id
    }

    fn id_mut(&mut self) -> &mut String {
        &mut self.id
    }

    fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    fn template_id(template: CloudAsrTemplate) -> &'static str {
        template.id
    }

    fn from_template(template: CloudAsrTemplate, id: &str, name: &str) -> Self {
        template.configuration(id, name)
    }

    /// A configuration made earlier carries a random suffix after the id of its template: a copy starts from the
    /// template.
    fn copy_id_base(&self) -> &str {
        self.id.split('-').next().unwrap_or(&self.id)
    }
}

pub type CloudAsrConfigurations = Configurations<CloudAsrConfiguration>;

impl Default for CloudAsrConfigurations {
    fn default() -> Self {
        let items: Vec<CloudAsrConfiguration> =
            CloudAsrTemplate::ALL.iter().map(|template| template.configuration(template.id, template.name)).collect();
        Self { selected: items[0].id.clone(), items }
    }
}

impl CloudAsrConfigurations {
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
