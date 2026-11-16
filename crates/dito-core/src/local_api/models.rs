//! The two models a client can name, and the installed ones a tool can list.

use serde::Serialize;

use crate::config::{Config, Source};

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
        serde_json::json!({
            "id": m.id, "role": m.role, "capabilities": m.capabilities, "formats": m.formats, "in_use": true
        })
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
