//! The two models a client can name, and the installed ones a tool can list.

use serde::Serialize;

use crate::chat::{CLOUD_THINKING, THINKING_OFF, THINKING_ON, thinking_choices};
use crate::config::{Config, Source};
use crate::provider::{Thinking, ThinkingCapabilities};

pub const TEXT_MODEL_ID: &str = "dito-text";
pub const SPEECH_MODEL_ID: &str = "dito-speech";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelRole {
    Text,
    Speech,
}

/// A model installed on this Mac, as the app's catalog knows it. The core cannot see the catalog, so the app lends it
/// through [`InstalledModels`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledModel {
    pub id: String,
    pub name: String,
    pub role: ModelRole,
}

pub trait InstalledModels: Send + Sync {
    fn installed(&self) -> Vec<InstalledModel>;

    /// What the catalog declares about a local text model's thinking.
    fn thinking(&self, _model_id: &str) -> Option<ThinkingCapabilities> {
        None
    }
}

/// The value a request sends to switch thinking off, as the OpenAI API names it.
const EFFORT_NONE: &str = "none";

fn text_thinking(cfg: &Config, installed: &dyn InstalledModels) -> Option<ThinkingCapabilities> {
    match cfg.text_model.source {
        Source::Local => installed.thinking(&cfg.text_model.local_model),
        Source::Provider => Some(CLOUD_THINKING),
    }
}

/// The `reasoning_effort` values the text model in use understands: `none` when it can stop thinking, then its
/// levels, or `on` for a model that only switches.
pub fn reasoning_efforts(cfg: &Config, installed: &dyn InstalledModels) -> Vec<&'static str> {
    text_thinking(cfg, installed)
        .map(|capabilities| {
            thinking_choices(&capabilities)
                .into_iter()
                .map(|choice| if choice == THINKING_OFF { EFFORT_NONE } else { choice })
                .collect()
        })
        .unwrap_or_default()
}

/// A request's `reasoning_effort` as the thinking choice of the text model in use. A cloud configuration gets the
/// value as it is, since its provider knows which efforts it takes. A local model that has no thinking control ignores
/// it, as clients send it by habit; one that only switches thinks for any effort but `none`; one with levels refuses a
/// level it does not have, with the list of those it does.
pub fn thinking_for(
    effort: Option<&str>,
    cfg: &Config,
    installed: &dyn InstalledModels,
) -> Result<Option<Thinking>, Vec<&'static str>> {
    let Some(effort) = effort.map(str::trim).filter(|effort| !effort.is_empty()) else { return Ok(None) };
    if cfg.text_model.source == Source::Provider {
        return Ok(Some(Thinking::Level(effort.to_owned())));
    }
    let Some(capabilities) = text_thinking(cfg, installed) else { return Ok(None) };
    if matches!(effort, EFFORT_NONE | THINKING_OFF | "minimal") {
        return Ok(capabilities.can_disable.then_some(Thinking::Off));
    }
    if capabilities.levels.is_empty() {
        return Ok(Some(Thinking::On));
    }
    match capabilities.levels.iter().find(|level| level.key == effort) {
        Some(level) => Ok(Some(Thinking::Level(level.key.to_owned()))),
        None if effort == THINKING_ON => Ok(Some(Thinking::On)),
        None => Err(reasoning_efforts(cfg, installed)),
    }
}

/// One entry of `GET /v1/models` and of the MCP model list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServedModel {
    pub id: String,
    pub role: ModelRole,
    pub capabilities: &'static [&'static str],
    pub formats: &'static [&'static str],
}

/// The text and the speech model Dito is set to. A local model is named by its catalog id. A cloud configuration is
/// named by its role, so the client never learns which provider or endpoint stands behind it (`design:tools/local-api`).
pub fn served_models(cfg: &Config) -> [ServedModel; 2] {
    let text = match cfg.text_model.source {
        Source::Local => cfg.text_model.local_model.clone(),
        Source::Provider => TEXT_MODEL_ID.to_owned(),
    };
    let speech = match cfg.asr.source {
        Source::Local => cfg.asr.local_model.clone(),
        Source::Provider => SPEECH_MODEL_ID.to_owned(),
    };
    [
        ServedModel { id: text, role: ModelRole::Text, capabilities: &["chat", "streaming"], formats: &["text"] },
        ServedModel { id: speech, role: ModelRole::Speech, capabilities: &["transcription"], formats: &["wav"] },
    ]
}

/// What `list_local_models` and the `dito://models` resource report: the models Dito is set to, then the other
/// installed local models.
pub fn model_listing(cfg: &Config, installed: &dyn InstalledModels) -> serde_json::Value {
    let served = served_models(cfg);
    let in_use: Vec<_> = served.iter().map(|m| m.id.as_str()).collect();
    let others: Vec<_> = installed
        .installed()
        .into_iter()
        .filter(|m| !in_use.contains(&m.id.as_str()))
        .map(|m| serde_json::json!({ "id": m.id, "name": m.name, "role": m.role, "in_use": false }))
        .collect();
    let in_use = served.iter().map(|m| {
        let mut entry = serde_json::json!({
            "id": m.id, "role": m.role, "capabilities": m.capabilities, "formats": m.formats, "in_use": true
        });
        if m.role == ModelRole::Text {
            entry["reasoning_efforts"] = reasoning_efforts(cfg, installed).into();
        }
        entry
    });
    serde_json::json!({ "models": in_use.chain(others).collect::<Vec<_>>() })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Installed(Vec<InstalledModel>);
    impl InstalledModels for Installed {
        fn installed(&self) -> Vec<InstalledModel> {
            self.0.clone()
        }
    }

    #[test]
    fn local_models_keep_their_catalog_id_and_cloud_ones_take_the_role_name() {
        let mut cfg = Config::default();
        let [text, speech] = served_models(&cfg);
        assert_eq!(
            (text.id.as_str(), speech.id.as_str()),
            (cfg.text_model.local_model.as_str(), cfg.asr.local_model.as_str())
        );
        cfg.text_model.source = Source::Provider;
        cfg.asr.source = Source::Provider;
        let [text, speech] = served_models(&cfg);
        assert_eq!((text.id.as_str(), speech.id.as_str()), (TEXT_MODEL_ID, SPEECH_MODEL_ID));
    }

    struct Thinks(ThinkingCapabilities);
    impl InstalledModels for Thinks {
        fn installed(&self) -> Vec<InstalledModel> {
            Vec::new()
        }
        fn thinking(&self, _: &str) -> Option<ThinkingCapabilities> {
            Some(self.0)
        }
    }

    #[test]
    fn a_reasoning_effort_becomes_what_the_model_in_use_can_do() {
        use crate::provider::ThinkingLevel;
        let mut cfg = Config::default();
        let none = Installed(Vec::new());
        assert_eq!(thinking_for(Some("high"), &cfg, &none), Ok(None), "a model without the control ignores it");
        assert_eq!(thinking_for(None, &cfg, &none), Ok(None));

        let switch = Thinks(ThinkingCapabilities { can_disable: true, levels: &[], default_level: None });
        assert_eq!(thinking_for(Some("none"), &cfg, &switch), Ok(Some(Thinking::Off)));
        assert_eq!(thinking_for(Some("high"), &cfg, &switch), Ok(Some(Thinking::On)));
        assert_eq!(reasoning_efforts(&cfg, &switch), ["none", "on"]);

        let levels = Thinks(ThinkingCapabilities {
            can_disable: true,
            levels: &[ThinkingLevel::LOW, ThinkingLevel::XHIGH],
            default_level: Some("xhigh"),
        });
        assert_eq!(thinking_for(Some("low"), &cfg, &levels), Ok(Some(Thinking::Level("low".into()))));
        assert_eq!(thinking_for(Some("high"), &cfg, &levels), Err(vec!["none", "low", "xhigh"]));

        cfg.text_model.source = Source::Provider;
        assert_eq!(thinking_for(Some("minimal"), &cfg, &none), Ok(Some(Thinking::Level("minimal".into()))));
        assert_eq!(reasoning_efforts(&cfg, &none), ["low", "medium", "high"]);
    }

    #[test]
    fn the_listing_puts_the_models_in_use_first_and_does_not_repeat_them() {
        let cfg = Config::default();
        let spare = InstalledModel { id: "spare".into(), name: "Spare".into(), role: ModelRole::Text };
        let used = InstalledModel { id: cfg.asr.local_model.clone(), name: "Used".into(), role: ModelRole::Speech };
        let listing = model_listing(&cfg, &Installed(vec![used, spare]));
        let ids: Vec<_> = listing["models"].as_array().unwrap().iter().map(|m| m["id"].as_str().unwrap()).collect();
        assert_eq!(ids, [cfg.text_model.local_model.as_str(), cfg.asr.local_model.as_str(), "spare"]);
        assert_eq!(listing["models"][2]["in_use"], false);
    }
}
