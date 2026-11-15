use std::borrow::Cow;

use dito_core::config::{Config, EngineConfig};
use dito_core::provider::configured_asr::CloudAsrConfigurations;
use dito_core::secrets::Role;

/// The cloud speech configurations in effect for `role`. Polish has none (it has its own configurations). Speech uses
/// the saved configurations or, before any are saved, the bundled defaults the pipeline falls back to.
pub fn cloud_asr_configurations(role: Role, engine: &EngineConfig) -> Option<Cow<'_, CloudAsrConfigurations>> {
    match (role, &engine.cloud_asr) {
        (Role::Text, _) => None,
        (Role::Asr, Some(configurations)) => Some(Cow::Borrowed(configurations)),
        (Role::Asr, None) => Some(Cow::Owned(CloudAsrConfigurations::default())),
    }
}

pub fn index(role: Role) -> usize {
    match role {
        Role::Asr => 0,
        Role::Text => 1,
    }
}

pub fn engine(cfg: &Config, role: Role) -> &EngineConfig {
    match role {
        Role::Asr => &cfg.asr,
        Role::Text => &cfg.text_model,
    }
}

pub fn engine_mut(cfg: &mut Config, role: Role) -> &mut EngineConfig {
    match role {
        Role::Asr => &mut cfg.asr,
        Role::Text => &mut cfg.text_model,
    }
}
