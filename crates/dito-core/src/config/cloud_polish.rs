//! Cloud polish configurations: the saved setups the user picks between. A template only pre-fills a new configuration
//! and fixes its field set; once made, a configuration is its own thing.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::ReasoningEffort;
use super::list::{Entry, List};
use crate::i18n::{Lang, Tr};
use crate::secrets::polish_account;

/// The API formats a new configuration can start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CloudPolishTemplate {
    Compat,
    Azure,
    Bedrock,
}

impl CloudPolishTemplate {
    /// In the order the Add menu lists them.
    pub const ALL: [Self; 3] = [Self::Compat, Self::Azure, Self::Bedrock];

    /// The name saved as a configuration's `template`, and the first part of the ids of configurations made from it.
    pub fn id(self) -> &'static str {
        match self {
            Self::Compat => "compat",
            Self::Azure => "azure",
            Self::Bedrock => "bedrock",
        }
    }

    pub fn label(self) -> Tr {
        match self {
            Self::Compat => Tr::ModelsOpenaiCompatible,
            Self::Azure => Tr::ModelsProviderAzure,
            Self::Bedrock => Tr::ModelsProviderBedrock,
        }
    }

    fn target(self) -> CloudPolishTarget {
        match self {
            Self::Compat => CloudPolishTarget::Compat { base_url: String::new(), model: String::new() },
            Self::Azure => CloudPolishTarget::Azure { endpoint: String::new(), deployment: String::new() },
            Self::Bedrock => CloudPolishTarget::Bedrock {
                profile: String::new(),
                region: "ap-northeast-1".into(),
                model_id: String::new(),
            },
        }
    }

    /// Azure and OpenAI-compatible servers start with the reasoning effort the settings always began with and nothing
    /// else. Bedrock starts with the sampling its engine used before it was configurable, and no reasoning effort.
    fn params(self) -> CloudPolishParams {
        match self {
            Self::Compat | Self::Azure => CloudPolishParams::default(),
            Self::Bedrock => CloudPolishParams {
                temperature: Some(0.2),
                max_tokens: Some(2048),
                reasoning_effort: ReasoningEffort::Custom(String::new()),
                ..CloudPolishParams::default()
            },
        }
    }

    /// A configuration of this template with the given identity.
    pub fn configuration(self, id: &str, name: &str) -> CloudPolishConfiguration {
        CloudPolishConfiguration { id: id.into(), name: name.into(), target: self.target(), params: self.params() }
    }
}

/// Where a configuration's requests go, and what they ask for: the fields its template fixes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "template", rename_all = "snake_case")]
pub enum CloudPolishTarget {
    Azure {
        #[serde(default)]
        endpoint: String,
        #[serde(default)]
        deployment: String,
    },
    Bedrock {
        #[serde(default)]
        profile: String,
        #[serde(default)]
        region: String,
        #[serde(default)]
        model_id: String,
    },
    Compat {
        #[serde(default)]
        base_url: String,
        #[serde(default)]
        model: String,
    },
}

impl CloudPolishTarget {
    pub fn template(&self) -> CloudPolishTemplate {
        match self {
            Self::Azure { .. } => CloudPolishTemplate::Azure,
            Self::Bedrock { .. } => CloudPolishTemplate::Bedrock,
            Self::Compat { .. } => CloudPolishTemplate::Compat,
        }
    }

    /// The first field of the form: the endpoint, region or base URL.
    pub fn address(&self) -> &str {
        match self {
            Self::Azure { endpoint, .. } => endpoint,
            Self::Bedrock { region, .. } => region,
            Self::Compat { base_url, .. } => base_url,
        }
    }

    pub fn address_mut(&mut self) -> &mut String {
        match self {
            Self::Azure { endpoint, .. } => endpoint,
            Self::Bedrock { region, .. } => region,
            Self::Compat { base_url, .. } => base_url,
        }
    }

    /// The model the requests ask for: the deployment name, model id or model name.
    pub fn model(&self) -> &str {
        match self {
            Self::Azure { deployment, .. } => deployment,
            Self::Bedrock { model_id, .. } => model_id,
            Self::Compat { model, .. } => model,
        }
    }

    pub fn model_mut(&mut self) -> &mut String {
        match self {
            Self::Azure { deployment, .. } => deployment,
            Self::Bedrock { model_id, .. } => model_id,
            Self::Compat { model, .. } => model,
        }
    }
}

/// What a request sends beyond the prompt. A parameter that is not set is left out of the request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CloudPolishParams {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    /// An empty `Custom` value is unset.
    pub reasoning_effort: ReasoningEffort,
    /// Merged into the request body last, for anything without a field of its own. Saved as a JSON string: TOML has
    /// no `null`.
    #[serde(with = "json_object", skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

/// One saved cloud setup: a name, the fields of its template, the request parameters and, under `account()`, a key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudPolishConfiguration {
    /// Stable identity; also the suffix of the configuration's Keychain account.
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub target: CloudPolishTarget,
    #[serde(default)]
    pub params: CloudPolishParams,
}

impl CloudPolishConfiguration {
    /// Keychain account of this configuration's API key.
    pub fn account(&self) -> String {
        polish_account(&self.id)
    }

    /// The engine's identity in history: `<template>:<model>`.
    pub fn label(&self) -> String {
        format!("{}:{}", self.target.template().id(), self.target.model())
    }
}

impl Entry for CloudPolishConfiguration {
    fn id(&self) -> &str {
        &self.id
    }

    fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }
}

/// The templates a fresh list starts from, in the order of the original provider tabs. The first one is in use.
const STARTING: [CloudPolishTemplate; 3] =
    [CloudPolishTemplate::Azure, CloudPolishTemplate::Bedrock, CloudPolishTemplate::Compat];

/// The cloud polish configurations and the one in use. Only `new` makes a list with content: its names depend on the
/// language the UI is shown in, which a default has no way to know.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CloudPolishConfigurations {
    /// Id of the configuration in use.
    pub selected: String,
    pub items: Vec<CloudPolishConfiguration>,
}

impl CloudPolishConfigurations {
    /// The three templates as configurations, named after their templates in `lang`. Their ids keep the Keychain
    /// accounts of the keys saved before configurations existed.
    pub fn new(lang: Lang) -> Self {
        let items: Vec<_> =
            STARTING.map(|template| template.configuration(template.id(), lang.tr(template.label()))).into();
        Self { selected: items[0].id.clone(), items }
    }

    /// The configuration in use while no list is saved, as `new` makes it but without a name, since no request reads
    /// one.
    pub(super) fn stand_in_active() -> CloudPolishConfiguration {
        let template = STARTING[0];
        template.configuration(template.id(), "")
    }

    fn list(&mut self) -> List<'_, CloudPolishConfiguration> {
        List::new(&mut self.selected, &mut self.items)
    }

    pub fn get(&self, id: &str) -> Option<&CloudPolishConfiguration> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut CloudPolishConfiguration> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    /// The configuration in use. `None` when no configuration has the selected id.
    pub fn active(&self) -> Option<&CloudPolishConfiguration> {
        self.get(&self.selected)
    }

    pub fn active_mut(&mut self) -> Option<&mut CloudPolishConfiguration> {
        let selected = &self.selected;
        self.items.iter_mut().find(|item| item.id == *selected)
    }

    /// Make `id` the configuration in use. An unknown id changes nothing.
    pub fn select(&mut self, id: &str) -> bool {
        self.list().select(id)
    }

    /// Append a configuration made from `template`, select it and return its id.
    pub fn add_from_template(&mut self, template: CloudPolishTemplate, name: &str) -> String {
        let id = self.list().fresh_id(template.id());
        self.list().push(template.configuration(&id, name));
        id
    }

    /// Append a copy of `id` under a new id, select it and return that id. The key is not copied: it belongs to the
    /// source's Keychain account.
    pub fn duplicate(&mut self, id: &str, name: &str) -> Option<String> {
        let mut copy = self.get(id)?.clone();
        copy.id = self.list().fresh_id(copy.target.template().id());
        copy.name = name.to_owned();
        let id = copy.id.clone();
        self.list().push(copy);
        Some(id)
    }

    /// Take a configuration out of the list and return it. Its key is still under its `account()`:
    /// `ConfigStore::remove_cloud_polish_configuration` removes both, deleting the key only once the list is saved. The last
    /// configuration stays, so this returns `None` for it. When the configuration in use goes, its neighbour is
    /// selected.
    pub fn remove(&mut self, id: &str) -> Option<CloudPolishConfiguration> {
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

/// `extra` is saved as a JSON string, and only an object is accepted back.
mod json_object {
    use serde::{Deserialize, Serialize};
    use serde_json::{Map, Value};

    pub fn serialize<S: serde::Serializer>(value: &Map<String, Value>, serializer: S) -> Result<S::Ok, S::Error> {
        serde_json::to_string(value).map_err(serde::ser::Error::custom)?.serialize(serializer)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Map<String, Value>, D::Error> {
        let text = String::deserialize(deserializer)?;
        serde_json::from_str(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::list::contract::{Editable, check_editing};

    impl Editable for CloudPolishConfigurations {
        fn ids(&self) -> Vec<String> {
            self.items.iter().map(|item| item.id.clone()).collect()
        }
        fn selected(&self) -> String {
            self.selected.clone()
        }
        fn name_of(&self, id: &str) -> Option<String> {
            self.get(id).map(|item| item.name.clone())
        }
        fn select(&mut self, id: &str) -> bool {
            CloudPolishConfigurations::select(self, id)
        }
        fn add(&mut self, name: &str) -> String {
            self.add_from_template(CloudPolishTemplate::Azure, name)
        }
        fn duplicate(&mut self, id: &str, name: &str) -> Option<String> {
            CloudPolishConfigurations::duplicate(self, id, name)
        }
        fn remove(&mut self, id: &str) -> bool {
            CloudPolishConfigurations::remove(self, id).is_some()
        }
        fn move_to(&mut self, id: &str, index: usize) -> bool {
            CloudPolishConfigurations::move_to(self, id, index)
        }
        fn rename(&mut self, id: &str, name: &str) -> bool {
            CloudPolishConfigurations::rename(self, id, name)
        }
    }

    #[test]
    fn the_editing_api_behaves_as_documented() {
        check_editing(&mut CloudPolishConfigurations::new(Lang::Ja));
    }

    #[test]
    fn a_fresh_list_is_the_three_templates_named_in_the_language() {
        let ja = CloudPolishConfigurations::new(Lang::Ja);
        let identities: Vec<_> = ja.items.iter().map(|c| (c.id.as_str(), c.name.as_str())).collect();
        assert_eq!(
            identities,
            [("azure", "Azure OpenAI"), ("bedrock", "Amazon Bedrock"), ("compat", "OpenAI 互換 API")]
        );
        assert_eq!(ja.selected, "azure");
        assert_eq!(CloudPolishConfigurations::new(Lang::En).items[2].name, "OpenAI-compatible API");
        assert_eq!(CloudPolishConfigurations::new(Lang::Zh).items[2].name, "OpenAI 兼容 API");

        let bedrock = ja.get("bedrock").unwrap();
        assert_eq!(bedrock.target.address(), "ap-northeast-1");
        assert_eq!(bedrock.params.temperature, Some(0.2));
        assert_eq!(bedrock.params.max_tokens, Some(2048));
        assert_eq!(bedrock.params.reasoning_effort.as_str(), "");
        for plain in [ja.get("azure").unwrap(), ja.get("compat").unwrap()] {
            assert_eq!(plain.target.address(), "");
            assert_eq!(plain.target.model(), "");
            let CloudPolishParams { temperature, top_p, max_tokens, extra, .. } = &plain.params;
            assert!(temperature.is_none() && top_p.is_none() && max_tokens.is_none() && extra.is_empty());
        }
    }

    #[test]
    fn templates_make_configurations_of_their_own_kind_with_account_and_label() {
        for template in CloudPolishTemplate::ALL {
            let made = template.configuration("x-1", "Name");
            assert_eq!(made.target.template(), template);
            assert_eq!(made.account(), "polish.x-1");
            let saved = toml::Table::try_from(&made).unwrap();
            assert_eq!(saved["template"].as_str(), Some(template.id()), "the saved tag is the template id");
        }
        let mut azure = CloudPolishTemplate::Azure.configuration("a", "A");
        *azure.target.model_mut() = "gpt-x".into();
        assert_eq!(azure.label(), "azure:gpt-x");
    }

    #[test]
    fn duplicating_copies_the_content_under_a_new_account() {
        let mut list = CloudPolishConfigurations::new(Lang::En);
        let source = list.get_mut("bedrock").unwrap();
        *source.target.model_mut() = "model-x".into();
        source.params.top_p = Some(0.9);
        let copy_id = list.duplicate("bedrock", "Copy").unwrap();
        let (source, copy) = (list.get("bedrock").unwrap(), list.get(&copy_id).unwrap());
        assert_ne!(copy.account(), source.account(), "the key stays with the source");
        assert_eq!((&copy.target, &copy.params), (&source.target, &source.params));
        assert!(copy_id.starts_with("bedrock-"));
    }

    #[test]
    fn configurations_survive_toml_with_their_parameters_and_extra_fields() {
        let mut list = CloudPolishConfigurations::new(Lang::Ja);
        let compat = list.get_mut("compat").unwrap();
        compat.params.temperature = Some(0.2);
        compat.params.top_p = Some(0.95);
        compat.params.max_tokens = Some(512);
        compat.params.reasoning_effort = "xhigh".to_owned().into();
        compat.params.extra = serde_json::from_str(r#"{"seed":42,"stop":null,"nested":{"a":[1,2]}}"#).unwrap();

        #[derive(Serialize, Deserialize)]
        struct Holder {
            cloud_polish: CloudPolishConfigurations,
        }
        let text = toml::to_string_pretty(&Holder { cloud_polish: list.clone() }).unwrap();
        assert!(text.contains("temperature = 0.2\n"), "an f32 is written as the number it was typed as:\n{text}");
        assert!(text.contains("top_p = 0.95\n"), "{text}");
        assert!(text.contains("max_tokens = 512\n"), "{text}");
        assert_eq!(text.matches("top_p").count(), 1, "a parameter that is not set is not written:\n{text}");
        assert_eq!(toml::from_str::<Holder>(&text).unwrap().cloud_polish, list);

        let saved = toml::Table::try_from(&list).unwrap();
        let extra = saved["items"][2]["params"]["extra"].as_str().expect("extra is saved as a JSON string");
        assert_eq!(serde_json::from_str::<Value>(extra).unwrap()["stop"], Value::Null, "a null survives");
        assert!(saved["items"][0]["params"].get("extra").is_none(), "an empty extra is not written");
    }

    #[test]
    fn extra_must_be_a_json_object_and_an_absent_one_is_empty() {
        let load = |extra: &str| {
            toml::from_str::<CloudPolishConfiguration>(&format!(
                "id = 'x'\nname = 'X'\ntemplate = 'compat'\n[params]\nextra = '{extra}'\n"
            ))
        };
        assert_eq!(load(r#"{"seed":1}"#).unwrap().params.extra["seed"], 1);
        for bad in ["[1]", "5", "not json", ""] {
            assert!(load(bad).is_err(), "{bad:?}");
        }
        let minimal: CloudPolishConfiguration = toml::from_str("id = 'x'\nname = 'X'\ntemplate = 'azure'\n").unwrap();
        assert_eq!(minimal.target, CloudPolishTarget::Azure { endpoint: String::new(), deployment: String::new() });
        assert_eq!(minimal.params, CloudPolishParams::default());
    }
}
