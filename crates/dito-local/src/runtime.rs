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
use dito_core::provider::{ChatMessage, ChatRole};

use crate::catalog::{ModelSpec, Sampling};
use crate::runtime_error::RuntimeError;

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

    /// Replies to a conversation, passing each piece to `on_piece` as it is decoded and returning their concatenation.
    /// Unlike [`LlmRuntime::generate`], a raised `stop` and an exhausted token budget both return the text so far, and
    /// `sampling.max_tokens` is the only cap, since a chat answer does not run about as long as its input. A runtime
    /// without streaming keeps this default and answers in one piece. `think` switches the model's thinking for this
    /// reply; unset keeps what the model was loaded with.
    fn chat(
        &self,
        messages: &[ChatMessage],
        sampling: &Sampling,
        _think: Option<bool>,
        on_piece: &mut dyn FnMut(&str),
        stop: &AtomicBool,
    ) -> Result<String, RuntimeError> {
        let system: Vec<&str> =
            messages.iter().filter(|m| m.role == ChatRole::System).map(|m| m.content.as_str()).collect();
        let mut dialogue = String::new();
        let turns: Vec<&ChatMessage> = messages.iter().filter(|m| m.role != ChatRole::System).collect();
        let (last, earlier) = turns.split_last().map(|(l, e)| (l.content.as_str(), e)).unwrap_or(("", &[]));
        for m in earlier {
            let who = if m.role == ChatRole::User { "User" } else { "Assistant" };
            dialogue.push_str(&format!("{who}: {}\n\n", m.content));
        }
        dialogue.push_str(last);
        let reply = match self.generate(&system.join("\n\n"), &[], &dialogue, sampling, stop) {
            Ok(reply) => reply.trim().to_string(),
            Err(RuntimeError::Cancelled) => return Ok(String::new()),
            Err(error) => return Err(error),
        };
        on_piece(&reply);
        Ok(reply)
    }

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
