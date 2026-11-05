//! Inference runtimes for the on-device models.
//!
//! | model files | runtime | feature |
//! | --- | --- | --- |
//! | SenseVoice (ONNX) | sherpa-onnx | `sherpa` |
//! | Qwen3 / Qwen3-ASR (GGUF + mmproj) | llama.cpp (Metal) | `llama` |

use std::path::Path;
use std::sync::Arc;

use dito_local::catalog::{ModelSpec, Runtime};
use dito_local::{AsrRuntime, LlmRuntime, RuntimeLoader};

#[cfg(feature = "llama")]
mod llama;
#[cfg(feature = "sherpa")]
mod sherpa;

/// Chooses the runtime from the model's catalog entry.
#[derive(Default)]
pub struct Runtimes;

impl RuntimeLoader for Runtimes {
    fn load_asr(&self, spec: &ModelSpec, dir: &Path) -> Result<Arc<dyn AsrRuntime>, String> {
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

    fn load_llm(&self, spec: &ModelSpec, dir: &Path) -> Result<Arc<dyn LlmRuntime>, String> {
        match spec.runtime {
            #[cfg(feature = "llama")]
            Runtime::LlamaCpp => Ok(Arc::new(llama::Chat::load(spec, dir)?)),
            other => Err(unavailable(other, &spec.name)),
        }
    }
}

fn unavailable(runtime: Runtime, name: &str) -> String {
    format!("{name}: the {runtime:?} runtime is not compiled into this build")
}

/// First `*.gguf` / named file inside a model directory.
#[allow(dead_code)]
fn find_file(dir: &Path, pred: impl Fn(&str) -> bool) -> Result<std::path::PathBuf, String> {
    std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(&pred))
        .ok_or_else(|| format!("no matching file in {}", dir.display()))
}
