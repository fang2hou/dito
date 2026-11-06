//! User-configurable synchronous ASR requests. Secrets are supplied at runtime only.

mod engine;
mod spec;
mod template;
mod validate;

pub use crate::config::AsrProfiles;
pub use engine::ConfiguredAsr;
pub use spec::configuration_spec;
pub use validate::validate;
