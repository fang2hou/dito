//! Dito core: OS-independent logic (config, prompts, providers, pipeline, stores).

pub mod audio;
pub mod config;
pub mod controller;
pub mod error;
pub mod factory;
pub mod health;
pub mod i18n;
pub mod identity;
pub mod insert;
pub mod pipeline;
pub mod prompts;
pub mod provider;
pub mod secrets;
pub mod store;

pub use audio::AudioClip;
pub use config::{Config, ConfigStore};
pub use error::{ErrorKind, ProviderError};
