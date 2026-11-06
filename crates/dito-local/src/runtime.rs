//! The seam between local model management and the inference runtimes (sherpa-onnx, llama.cpp).
//!
//! Runtimes are plugged in through [`RuntimeLoader`]. Without one, loading fails with a clear reason and the status
//! dot turns red; it never fakes a result.

use std::path::Path;
use std::sync::Arc;

use crate::catalog::ModelSpec;
use crate::runtime_error::RuntimeError;

/// Recognized text and, when the engine reports it, the detected language code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrOutput {
    pub text: String,
    pub language: Option<String>,
}

pub trait AsrRuntime: Send + Sync {
    fn transcribe(
        &self,
        samples: &[f32],
        language: Option<&str>,
        hotwords: &[String],
    ) -> Result<AsrOutput, RuntimeError>;
}

pub trait LlmRuntime: Send + Sync {
    fn generate(&self, system: &str, user: &str) -> Result<String, RuntimeError>;
}

pub trait RuntimeLoader: Send + Sync {
    fn load_asr(&self, spec: &ModelSpec, dir: &Path) -> Result<Arc<dyn AsrRuntime>, RuntimeError>;
    fn load_llm(&self, spec: &ModelSpec, dir: &Path) -> Result<Arc<dyn LlmRuntime>, RuntimeError>;
}

/// Loader used when no inference runtime is compiled in.
pub struct NoRuntime;

impl RuntimeLoader for NoRuntime {
    fn load_asr(&self, spec: &ModelSpec, _: &Path) -> Result<Arc<dyn AsrRuntime>, RuntimeError> {
        Err(RuntimeError::NotBuiltIn(spec.runtime))
    }
    fn load_llm(&self, spec: &ModelSpec, _: &Path) -> Result<Arc<dyn LlmRuntime>, RuntimeError> {
        Err(RuntimeError::NotBuiltIn(spec.runtime))
    }
}
