use std::borrow::Cow;

use dito_core::config::{Config, EngineConfig};
use dito_core::provider::configured_asr::AsrProfiles;
use dito_core::secrets::Role;

/// The cloud speech profiles in effect for `role`. Polish has none (it has its own configurations). Speech uses the
/// saved profiles or, before any are saved, the bundled defaults the pipeline falls back to.
pub fn cloud_profiles(role: Role, engine: &EngineConfig) -> Option<Cow<'_, AsrProfiles>> {
    match (role, &engine.cloud_asr) {
        (Role::Polish, _) => None,
        (Role::Asr, Some(profiles)) => Some(Cow::Borrowed(profiles)),
        (Role::Asr, None) => Some(Cow::Owned(AsrProfiles::default())),
    }
}

pub fn index(role: Role) -> usize {
    match role {
        Role::Asr => 0,
        Role::Polish => 1,
    }
}

pub fn engine(cfg: &Config, role: Role) -> &EngineConfig {
    match role {
        Role::Asr => &cfg.asr,
        Role::Polish => &cfg.polish.engine,
    }
}

pub fn engine_mut(cfg: &mut Config, role: Role) -> &mut EngineConfig {
    match role {
        Role::Asr => &mut cfg.asr,
        Role::Polish => &mut cfg.polish.engine,
    }
}
