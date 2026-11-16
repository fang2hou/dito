//! Persisted settings.

mod chat;
mod cloud_asr;
mod cloud_text;
mod data_dir;
mod engine;
mod error;
mod general;
mod hud;
mod list;
mod local_api;
mod migrate;
mod restore;
mod settings;
mod store;
mod transcription;

pub use chat::ChatSettings;
pub use cloud_asr::{CloudAsrConfiguration, CloudAsrConfigurations, CloudAsrTemplate};
pub use cloud_text::{
    CloudTextConfiguration, CloudTextConfigurations, CloudTextParams, CloudTextTarget, CloudTextTemplate,
};
pub use data_dir::default_data_dir;
pub use engine::{
    DEFAULT_ASR_LOCAL, DEFAULT_TEXT_LOCAL, EngineConfig, EngineLabels, LocalTextParams, PolishConfig, ReasoningEffort,
    Source,
};
pub use error::ConfigError;
pub use general::{
    Appearance, DEFAULT_SHORTCUT, DEFAULT_TOOLS_SHORTCUT, DisplayLanguage, General, InsertMethod, MusicMuteMode,
    PreferredLanguage, RecordMode,
};
pub use hud::HudPosition;
pub use local_api::{DEFAULT_LOCAL_API_PORT, LocalApiSettings};
pub use migrate::SCHEMA_VERSION;
pub use restore::{ParseProblem, parse_problem, restore_defaults};
pub use settings::Config;
pub use store::ConfigStore;
pub use transcription::{DEFAULT_TITLE_TEMPLATE, TranscriptionSettings};
