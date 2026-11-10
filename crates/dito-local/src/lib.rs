//! On-device models: catalog, downloads, and the engines that run them.

pub mod catalog;
mod disk;
pub mod download;
pub mod local_engines;
pub mod manager;
mod manager_error;
pub mod runtime;
mod runtime_error;

pub use catalog::{ModelKind, ModelSpec, Runtime, Sampling, default_catalog};
pub use local_engines::LocalEngines;
pub use manager::{ModelManager, ModelState, Snapshot, Storage};
pub use manager_error::ManagerError;
pub use runtime::{AsrOutput, AsrRuntime, LlmRuntime, NoRuntime, RuntimeLoader};
pub use runtime_error::RuntimeError;
