//! Saved cloud polish configurations. A template only pre-fills a new configuration and fixes its field set.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::list::{Configurations, Entry};
use super::{ReasoningEffort, json_string};
use crate::i18n::{Lang, Tr};
use crate::secrets::text_account;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CloudTextTemplate {
    Compat,
    Azure,
    Bedrock,
}

impl CloudTextTemplate {
    /// Add-menu order.
    pub const ALL: [Self; 3] = [Self::Compat, Self::Azure, Self::Bedrock];

    /// Saved as a configuration's `template` and the first part of the ids made from it.
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
            Self::Azure => Tr::ModelsAzureOpenai,
            Self::Bedrock => Tr::ModelsAmazonBedrock,
        }
    }

    fn target(self) -> CloudTextTarget {
        match self {
            Self::Compat => CloudTextTarget::Compat { base_url: String::new(), model: String::new() },
            Self::Azure => CloudTextTarget::Azure { endpoint: String::new(), deployment: String::new() },
            Self::Bedrock => CloudTextTarget::Bedrock {
                profile: String::new(),
                region: "ap-northeast-1".into(),
                model_id: String::new(),
            },
        }
    }

    fn params(self) -> CloudTextParams {
        match self {
            Self::Compat | Self::Azure => CloudTextParams::default(),
            Self::Bedrock => CloudTextParams {
                temperature: Some(0.2),
                max_tokens: Some(2048),
                reasoning_effort: ReasoningEffort::Custom(String::new()),
                ..CloudTextParams::default()
            },
        }
    }

    pub fn configuration(self, id: &str, name: &str) -> CloudTextConfiguration {
        CloudTextConfiguration { id: id.into(), name: name.into(), target: self.target(), params: self.params() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "template", rename_all = "snake_case")]
pub enum CloudTextTarget {
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

impl CloudTextTarget {
    pub fn template(&self) -> CloudTextTemplate {
        match self {
            Self::Azure { .. } => CloudTextTemplate::Azure,
            Self::Bedrock { .. } => CloudTextTemplate::Bedrock,
            Self::Compat { .. } => CloudTextTemplate::Compat,
        }
    }

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
pub struct CloudTextParams {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    /// An empty `Custom` value is unset.
    pub reasoning_effort: ReasoningEffort,
    /// Merged into the request body last. Saved as a JSON string: TOML has no `null`. Only an object is accepted back.
    #[serde(with = "json_string", skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudTextConfiguration {
    /// Stable; also the suffix of the Keychain account.
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub target: CloudTextTarget,
    #[serde(default)]
    pub params: CloudTextParams,
}

impl CloudTextConfiguration {
    pub fn account(&self) -> String {
        text_account(&self.id)
    }

    /// Persisted in history as `<template>:<model>`.
    pub fn label(&self) -> String {
        format!("{}:{}", self.target.template().id(), self.target.model())
    }
}

impl Entry for CloudTextConfiguration {
    type Template = CloudTextTemplate;

    fn id(&self) -> &str {
        &self.id
    }

    fn id_mut(&mut self) -> &mut String {
        &mut self.id
    }

    fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    fn template_id(template: CloudTextTemplate) -> &'static str {
        template.id()
    }

    fn from_template(template: CloudTextTemplate, id: &str, name: &str) -> Self {
        template.configuration(id, name)
    }

    fn copy_id_base(&self) -> &str {
        self.target.template().id()
    }
}

/// The templates a fresh list starts from; the first is in use.
const STARTING: [CloudTextTemplate; 3] =
    [CloudTextTemplate::Azure, CloudTextTemplate::Bedrock, CloudTextTemplate::Compat];

/// The cloud polish configurations and the one in use. Only `new` makes a list with content: its names depend on the
/// language the UI is shown in, which a default has no way to know.
pub type CloudTextConfigurations = Configurations<CloudTextConfiguration>;

impl Default for CloudTextConfigurations {
    fn default() -> Self {
        Self { selected: String::new(), items: Vec::new() }
    }
}

impl CloudTextConfigurations {
    /// The three templates as configurations, named after their templates in `lang`. Their ids are the ones the
    /// migration from version 2 gives the old forms, so a fresh list and a migrated one use the same Keychain accounts.
    pub fn new(lang: Lang) -> Self {
        let items: Vec<_> =
            STARTING.map(|template| template.configuration(template.id(), lang.tr(template.label()))).into();
        Self { selected: items[0].id.clone(), items }
    }

    /// The configuration in use while no list is saved, as `new` makes it but without a name, since no request reads
    /// one.
    pub(super) fn stand_in_active() -> CloudTextConfiguration {
        let template = STARTING[0];
        template.configuration(template.id(), "")
    }

    pub fn active_mut(&mut self) -> Option<&mut CloudTextConfiguration> {
        let selected = &self.selected;
        self.items.iter_mut().find(|item| item.id == *selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::list::contract::{Editable, check_editing};

    impl Editable for CloudTextConfigurations {
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
            CloudTextConfigurations::select(self, id)
        }
        fn add(&mut self, name: &str) -> String {
            self.add_from_template(CloudTextTemplate::Azure, name)
        }
        fn duplicate(&mut self, id: &str, name: &str) -> Option<String> {
            CloudTextConfigurations::duplicate(self, id, name)
        }
        fn remove(&mut self, id: &str) -> bool {
            CloudTextConfigurations::remove(self, id).is_some()
        }
        fn move_to(&mut self, id: &str, index: usize) -> bool {
            CloudTextConfigurations::move_to(self, id, index)
        }
        fn rename(&mut self, id: &str, name: &str) -> bool {
            CloudTextConfigurations::rename(self, id, name)
        }
    }

    #[test]
    fn the_editing_api_behaves_as_documented() {
        check_editing(&mut CloudTextConfigurations::new(Lang::Ja));
    }

    #[test]
    fn a_fresh_list_is_the_three_templates_named_in_the_language() {
        let ja = CloudTextConfigurations::new(Lang::Ja);
        let identities: Vec<_> = ja.items.iter().map(|c| (c.id.as_str(), c.name.as_str())).collect();
        assert_eq!(
            identities,
            [("azure", "Azure OpenAI"), ("bedrock", "Amazon Bedrock"), ("compat", "OpenAI 互換 API")]
        );
        assert_eq!(ja.selected, "azure");
        assert_eq!(CloudTextConfigurations::new(Lang::En).items[2].name, "OpenAI-compatible API");
        assert_eq!(CloudTextConfigurations::new(Lang::Zh).items[2].name, "OpenAI 兼容 API");

        let bedrock = ja.get("bedrock").unwrap();
        assert_eq!(bedrock.target.address(), "ap-northeast-1");
        assert_eq!(bedrock.params.temperature, Some(0.2));
        assert_eq!(bedrock.params.max_tokens, Some(2048));
        assert_eq!(bedrock.params.reasoning_effort.as_str(), "");
        for plain in [ja.get("azure").unwrap(), ja.get("compat").unwrap()] {
            assert_eq!(plain.target.address(), "");
            assert_eq!(plain.target.model(), "");
            let CloudTextParams { temperature, top_p, max_tokens, extra, .. } = &plain.params;
            assert!(temperature.is_none() && top_p.is_none() && max_tokens.is_none() && extra.is_empty());
        }
    }

    #[test]
    fn templates_make_configurations_of_their_own_kind_with_account_and_label() {
        for template in CloudTextTemplate::ALL {
            let made = template.configuration("x-1", "Name");
            assert_eq!(made.target.template(), template);
            assert_eq!(made.account(), "polish.x-1");
            let saved = toml::Table::try_from(&made).unwrap();
            assert_eq!(saved["template"].as_str(), Some(template.id()), "the saved tag is the template id");
        }
        let mut azure = CloudTextTemplate::Azure.configuration("a", "A");
        *azure.target.model_mut() = "gpt-x".into();
        assert_eq!(azure.label(), "azure:gpt-x");
    }

    #[test]
    fn duplicating_copies_the_content_under_a_new_account() {
        let mut list = CloudTextConfigurations::new(Lang::En);
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
        let mut list = CloudTextConfigurations::new(Lang::Ja);
        let compat = list.get_mut("compat").unwrap();
        compat.params.temperature = Some(0.2);
        compat.params.top_p = Some(0.95);
        compat.params.max_tokens = Some(512);
        compat.params.reasoning_effort = "xhigh".to_owned().into();
        compat.params.extra = serde_json::from_str(r#"{"seed":42,"stop":null,"nested":{"a":[1,2]}}"#).unwrap();

        #[derive(Serialize, Deserialize)]
        struct Holder {
            cloud_text: CloudTextConfigurations,
        }
        let text = toml::to_string_pretty(&Holder { cloud_text: list.clone() }).unwrap();
        assert!(text.contains("temperature = 0.2\n"), "an f32 is written as the number it was typed as:\n{text}");
        assert!(text.contains("top_p = 0.95\n"), "{text}");
        assert!(text.contains("max_tokens = 512\n"), "{text}");
        assert_eq!(text.matches("top_p").count(), 1, "a parameter that is not set is not written:\n{text}");
        assert_eq!(toml::from_str::<Holder>(&text).unwrap().cloud_text, list);

        let saved = toml::Table::try_from(&list).unwrap();
        let extra = saved["items"][2]["params"]["extra"].as_str().expect("extra is saved as a JSON string");
        assert_eq!(serde_json::from_str::<Value>(extra).unwrap()["stop"], Value::Null, "a null survives");
        assert!(saved["items"][0]["params"].get("extra").is_none(), "an empty extra is not written");
    }

    #[test]
    fn extra_must_be_a_json_object_and_an_absent_one_is_empty() {
        let load = |extra: &str| {
            toml::from_str::<CloudTextConfiguration>(&format!(
                "id = 'x'\nname = 'X'\ntemplate = 'compat'\n[params]\nextra = '{extra}'\n"
            ))
        };
        assert_eq!(load(r#"{"seed":1}"#).unwrap().params.extra["seed"], 1);
        for bad in ["[1]", "5", "not json", ""] {
            assert!(load(bad).is_err(), "{bad:?}");
        }
        let minimal: CloudTextConfiguration = toml::from_str("id = 'x'\nname = 'X'\ntemplate = 'azure'\n").unwrap();
        assert_eq!(minimal.target, CloudTextTarget::Azure { endpoint: String::new(), deployment: String::new() });
        assert_eq!(minimal.params, CloudTextParams::default());
    }
}
