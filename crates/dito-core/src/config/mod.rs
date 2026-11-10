//! Settings: the schema, its versioned migrations, the persisted store and the default data directory.

mod asr_profiles;
mod data_dir;
mod engine;
mod error;
mod general;
mod hud;
mod list;
mod migrate;
mod polish_configurations;
mod settings;
mod store;

pub use asr_profiles::{AsrProfile, AsrProfiles, AsrTemplate};
pub use data_dir::default_data_dir;
pub use engine::{
    AzureSettings, BedrockSettings, CompatSettings, DEFAULT_ASR_LOCAL, DEFAULT_POLISH_LOCAL, EngineConfig,
    EngineLabels, PolishConfig, ProviderKind, ProviderSettings, ReasoningEffort, Source,
};
pub use error::ConfigError;
pub use general::{
    Appearance, DEFAULT_SHORTCUT, DisplayLanguage, General, InsertMethod, MusicMuteMode, PreferredLanguage, RecordMode,
};
pub use hud::HudPosition;
pub use migrate::SCHEMA_VERSION;
pub use polish_configurations::{
    PolishConfiguration, PolishConfigurations, PolishTarget, PolishTemplate, RequestParams,
};
pub use settings::Config;
pub use store::ConfigStore;
