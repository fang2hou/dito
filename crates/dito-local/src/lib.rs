//! On-device models: catalog, downloads, and the engines that run them.

pub mod catalog;
mod disk;
pub mod download;
pub mod intent;
pub mod local_engines;
pub mod managed_process;
pub mod manager;
mod manager_error;
pub mod manifest;
mod process;
pub mod runtime;
mod runtime_error;

pub use catalog::{ModelKind, ModelSpec, Runtime, SILERO_VAD, Sampling, default_catalog};
pub use local_engines::{EngineUsage, LocalEngines};
pub use manager::{ModelManager, ModelState, Snapshot, Storage};
pub use manager_error::ManagerError;
pub use process::{ProcessUsage, process_usage, process_usage_for};
pub use runtime::{AsrOutput, AsrRuntime, Example, LlmRuntime, NoRuntime, RuntimeLoader};
pub use runtime_error::RuntimeError;
