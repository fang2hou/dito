use dito_core::local_api::{InstalledModel, InstalledModels, ModelRole};
use dito_core::provider::ThinkingCapabilities;
use dito_local::{ModelKind, ModelManager};

/// The installed local models, as the Local API lists them.
pub struct InstalledCatalog(pub ModelManager);

impl InstalledModels for InstalledCatalog {
    fn installed(&self) -> Vec<InstalledModel> {
        self.0
            .catalog()
            .iter()
            .filter(|spec| self.0.is_installed(&spec.id))
            .filter_map(|spec| {
                let role = match spec.kind {
                    ModelKind::Text => ModelRole::Text,
                    ModelKind::Asr => ModelRole::Speech,
                    ModelKind::Vad => return None,
                };
                Some(InstalledModel { id: spec.id.clone(), name: spec.name.clone(), role })
            })
            .collect()
    }

    fn thinking(&self, model_id: &str) -> Option<ThinkingCapabilities> {
        self.0.spec(model_id)?.text_capabilities?.thinking
    }
}
