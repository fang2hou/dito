use dito_core::config::{Config, EngineConfig};
use dito_core::secrets::Role;

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
