//! The seam between local model management and the inference runtimes (sherpa-onnx, llama.cpp).
//!
//! Without a [`RuntimeLoader`], loading fails with a clear reason and the status dot turns red; it never fakes a
//! result.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use dito_core::intent::{Reading, Request};
/// A demonstration turn of a polish request, as the engines receive it.
pub use dito_core::provider::Example;

use crate::catalog::{ModelSpec, Sampling};
use crate::runtime_error::RuntimeError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrOutput {
    pub text: String,
    pub language: Option<String>,
}

pub trait AsrRuntime: Send + Sync {
    /// Process-backed engines release their worker even when a caller still holds an engine handle.
    fn shutdown(&self) {}

    fn failure(&self) -> Option<String> {
        None
    }

    fn process_id(&self) -> Option<u32> {
        None
    }

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
    /// A runtime must not run past `sampling.max_tokens`, which caps the budget it computes from the input. `stop`
    /// works as in [`AsrRuntime::transcribe`]. When the token budget runs out before the model ends its turn, return
    /// [`RuntimeError::Truncated`], never the cut-off text. `examples` are earlier turns of the conversation, sent
    /// between the system prompt and `user`.
    fn generate(
        &self,
        system: &str,
        examples: &[Example],
        user: &str,
        sampling: &Sampling,
        stop: &AtomicBool,
    ) -> Result<String, RuntimeError>;

    /// One prompt pass with nothing generated: the logits of the request's options at the answer position and the
    /// hidden states it names (see [`dito_core::intent`]). `stop` works as in [`AsrRuntime::transcribe`]. A runtime
    /// that cannot read its model this way keeps this default and returns [`RuntimeError::ReadUnsupported`].
    fn read(&self, _request: &Request, _stop: &AtomicBool) -> Result<Reading, RuntimeError> {
        Err(RuntimeError::ReadUnsupported)
    }
}

pub trait RuntimeLoader: Send + Sync {
    fn load_asr(&self, spec: &ModelSpec, dir: &Path) -> Result<Arc<dyn AsrRuntime>, RuntimeError>;
    fn load_llm(&self, spec: &ModelSpec, dir: &Path) -> Result<Arc<dyn LlmRuntime>, RuntimeError>;

    fn load_asr_cancellable(
        &self,
        spec: &ModelSpec,
        dir: &Path,
        _stop: &AtomicBool,
    ) -> Result<Arc<dyn AsrRuntime>, RuntimeError> {
        self.load_asr(spec, dir)
    }
}

pub struct NoRuntime;

impl RuntimeLoader for NoRuntime {
    fn load_asr(&self, spec: &ModelSpec, _: &Path) -> Result<Arc<dyn AsrRuntime>, RuntimeError> {
        Err(RuntimeError::NotBuiltIn(spec.format.runtime()))
    }
    fn load_llm(&self, spec: &ModelSpec, _: &Path) -> Result<Arc<dyn LlmRuntime>, RuntimeError> {
        Err(RuntimeError::NotBuiltIn(spec.format.runtime()))
    }
}
