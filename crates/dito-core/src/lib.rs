//! Dito core: OS-independent logic (config, prompts, providers, pipeline, stores).

pub mod asr_compare;
pub mod audio;
pub mod chat;
pub mod command;
pub mod config;
pub mod controller;
pub mod error;
pub mod factory;
pub mod health;
pub mod i18n;
pub mod identity;
pub mod insert;
pub mod intent;
pub mod local_api;
pub mod meeting_notes;
pub mod pipeline;
pub mod prompts;
pub mod provider;
pub mod secrets;
pub mod spoken_forms;
pub mod store;
pub mod surrounding;
pub mod time;
pub mod trace;
pub mod transcription;

pub use time::unix_seconds;
pub mod word_finder;

pub use audio::AudioClip;
pub use config::{Config, ConfigStore};
pub use error::{ErrorKind, ProviderError};
