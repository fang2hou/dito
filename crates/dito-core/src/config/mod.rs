//! Settings: the schema, its versioned migrations, the persisted store and the default data directory.

mod asr_profiles;
mod data_dir;
mod engine;
mod error;
mod general;
mod hud;
mod migrate;
mod settings;
mod store;

pub use asr_profiles::AsrProfiles;
pub use data_dir::default_data_dir;
pub use engine::{
    AzureSettings, BedrockSettings, CompatSettings, DEFAULT_ASR_LOCAL, DEFAULT_POLISH_LOCAL, EngineConfig,
    PolishConfig, ProviderKind, ProviderSettings, ReasoningEffort, Source,
};
pub use error::ConfigError;
pub use general::{
    Appearance, DEFAULT_SHORTCUT, DisplayLanguage, General, InsertMethod, MusicMuteMode, PreferredLanguage, RecordMode,
};
pub use hud::HudPosition;
pub use migrate::SCHEMA_VERSION;
pub use settings::Config;
pub use store::ConfigStore;
