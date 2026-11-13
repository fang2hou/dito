//! Persisted settings.

mod cloud_asr;
mod cloud_polish;
mod data_dir;
mod engine;
mod error;
mod general;
mod hud;
mod list;
mod migrate;
mod restore;
mod settings;
mod store;
mod transcription;

pub use cloud_asr::{CloudAsrConfiguration, CloudAsrConfigurations, CloudAsrTemplate};
pub use cloud_polish::{
    CloudPolishConfiguration, CloudPolishConfigurations, CloudPolishParams, CloudPolishTarget, CloudPolishTemplate,
};
pub use data_dir::default_data_dir;
pub use engine::{
    DEFAULT_ASR_LOCAL, DEFAULT_POLISH_LOCAL, EngineConfig, EngineLabels, LocalPolishParams, PolishConfig,
    ReasoningEffort, Source,
};
pub use error::ConfigError;
pub use general::{
    Appearance, DEFAULT_SHORTCUT, DisplayLanguage, General, InsertMethod, MusicMuteMode, PreferredLanguage, RecordMode,
};
pub use hud::HudPosition;
pub use migrate::SCHEMA_VERSION;
pub use restore::{ParseProblem, parse_problem, restore_defaults};
pub use settings::Config;
pub use store::ConfigStore;
pub use transcription::TranscriptionSettings;
