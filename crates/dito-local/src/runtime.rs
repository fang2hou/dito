//! The seam between local model management and the inference runtimes (sherpa-onnx, llama.cpp).
//!
//! Runtimes are plugged in through [`RuntimeLoader`]. Without one, loading fails with a clear reason and the status
//! dot turns red; it never fakes a result.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::catalog::ModelSpec;
use crate::runtime_error::RuntimeError;

/// Recognized text and, when the engine reports it, the detected language code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrOutput {
    pub text: String,
    pub language: Option<String>,
}

pub trait AsrRuntime: Send + Sync {
    /// `stop` is raised when the caller no longer wants the result, for example when the dictation is cancelled. Check
    /// it between decoding steps with `Ordering::Relaxed` and return [`RuntimeError::Cancelled`] at once: a runtime
    /// that ignores it keeps the model busy to the end, and the next dictation waits behind the abandoned one.
    fn transcribe(
        &self,
        samples: &[f32],
        language: Option<&str>,
        hotwords: &[String],
        stop: &AtomicBool,
    ) -> Result<AsrOutput, RuntimeError>;
}

pub trait LlmRuntime: Send + Sync {
    /// `stop` works as in [`AsrRuntime::transcribe`]. When the token budget runs out before the model ends its turn,
    /// return [`RuntimeError::Truncated`], never the cut-off text.
    fn generate(&self, system: &str, user: &str, stop: &AtomicBool) -> Result<String, RuntimeError>;
}

pub trait RuntimeLoader: Send + Sync {
    fn load_asr(&self, spec: &ModelSpec, dir: &Path) -> Result<Arc<dyn AsrRuntime>, RuntimeError>;
    fn load_llm(&self, spec: &ModelSpec, dir: &Path) -> Result<Arc<dyn LlmRuntime>, RuntimeError>;
}

/// Loader used when no inference runtime is compiled in.
pub struct NoRuntime;

impl RuntimeLoader for NoRuntime {
    fn load_asr(&self, spec: &ModelSpec, _: &Path) -> Result<Arc<dyn AsrRuntime>, RuntimeError> {
        Err(RuntimeError::NotBuiltIn(spec.format.runtime()))
    }
    fn load_llm(&self, spec: &ModelSpec, _: &Path) -> Result<Arc<dyn LlmRuntime>, RuntimeError> {
        Err(RuntimeError::NotBuiltIn(spec.format.runtime()))
    }
}
