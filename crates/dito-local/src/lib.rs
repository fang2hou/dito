//! On-device models: catalog, downloads, and the engines that run them.

pub mod catalog;
pub mod download;
pub mod engine;
pub mod manager;

pub use catalog::{ModelKind, ModelSpec, Runtime, default_catalog};
pub use engine::{AsrOutput, AsrRuntime, LlmRuntime, LocalEngines, RuntimeLoader};
pub use manager::{ModelManager, ModelState, Snapshot, Storage};
