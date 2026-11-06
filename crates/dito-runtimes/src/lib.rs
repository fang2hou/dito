//! Inference runtimes for the on-device models.
//!
//! | model files | runtime | feature |
//! | --- | --- | --- |
//! | SenseVoice (ONNX) | sherpa-onnx | `sherpa` |
//! | Qwen3 / Qwen3-ASR (GGUF + mmproj) | llama.cpp (Metal) | `llama` |

use std::path::Path;
use std::sync::Arc;

use dito_local::catalog::{ModelSpec, Runtime};
use dito_local::{AsrRuntime, LlmRuntime, RuntimeError, RuntimeLoader};

#[cfg(any(feature = "sherpa", test))]
#[cfg_attr(not(feature = "sherpa"), allow(dead_code))]
mod chunking;
#[cfg(feature = "llama")]
mod llama;
#[cfg(any(feature = "llama", test))]
#[cfg_attr(not(feature = "llama"), allow(dead_code))]
mod qwen_text;
#[cfg(feature = "sherpa")]
mod sherpa;

/// Chooses the runtime from the model's catalog entry.
#[derive(Default)]
pub struct Runtimes;

impl RuntimeLoader for Runtimes {
    #[cfg_attr(not(any(feature = "llama", feature = "sherpa")), allow(unused_variables))]
    fn load_asr(&self, spec: &ModelSpec, dir: &Path) -> Result<Arc<dyn AsrRuntime>, RuntimeError> {
        match spec.runtime {
            #[cfg(feature = "sherpa")]
            Runtime::SherpaOnnx => Ok(Arc::new(sherpa::SenseVoice::load(dir)?)),
            #[cfg(feature = "llama")]
            Runtime::LlamaCpp => Ok(Arc::new(llama::Asr::load(dir)?)),
            // Reachable only when a runtime feature is compiled out; with all features on,
            // both variants are handled above and this arm is unreachable.
            #[allow(unreachable_patterns)]
            other => Err(unavailable(other, &spec.name)),
        }
    }

    #[cfg_attr(not(feature = "llama"), allow(unused_variables))]
    fn load_llm(&self, spec: &ModelSpec, dir: &Path) -> Result<Arc<dyn LlmRuntime>, RuntimeError> {
        match spec.runtime {
            #[cfg(feature = "llama")]
            Runtime::LlamaCpp => Ok(Arc::new(llama::Chat::load(spec, dir)?)),
            other => Err(unavailable(other, &spec.name)),
        }
    }
}

fn unavailable(runtime: Runtime, name: &str) -> RuntimeError {
    RuntimeError::NotCompiled { name: name.to_string(), runtime }
}

/// The first file in a model directory whose name satisfies `pred`.
#[cfg(any(feature = "llama", feature = "sherpa"))]
fn find_file(dir: &Path, pred: impl Fn(&str) -> bool) -> Result<std::path::PathBuf, RuntimeError> {
    std::fs::read_dir(dir)
        .map_err(|source| RuntimeError::ReadDir { dir: dir.to_path_buf(), source })?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(&pred))
        .ok_or_else(|| RuntimeError::NoMatchingFile(dir.to_path_buf()))
}
