//! Inference runtimes for the on-device models.
//!
//! | model files | runtime | feature |
//! | --- | --- | --- |
//! | SenseVoice, NeMo TDT (ONNX) | sherpa-onnx | `sherpa` |
//! | Qwen3-ASR (GGUF + mmproj) | llama.cpp (Metal) | `llama` |
//! | Chat models (GGUF, prompt from its chat template, `disable_thinking` is the Qwen think prefill) | llama.cpp (Metal) | `llama` |

use std::path::Path;
use std::sync::Arc;

use dito_local::catalog::{ModelFormat, ModelSpec, Runtime};
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
        match spec.format {
            #[cfg(feature = "sherpa")]
            ModelFormat::SenseVoice { model, tokens } => {
                Ok(Arc::new(sherpa::SenseVoice::load(&model_file(dir, model)?, &model_file(dir, tokens)?)?))
            }
            #[cfg(feature = "sherpa")]
            ModelFormat::NemoTdt { encoder, decoder, joiner, tokens, feature_dim } => {
                Ok(Arc::new(sherpa::NemoTdt::load(
                    &model_file(dir, encoder)?,
                    &model_file(dir, decoder)?,
                    &model_file(dir, joiner)?,
                    &model_file(dir, tokens)?,
                    feature_dim,
                )?))
            }
            #[cfg(feature = "llama")]
            ModelFormat::QwenAsr { model, projector } => {
                Ok(Arc::new(llama::Asr::load(&model_file(dir, model)?, &model_file(dir, projector)?)?))
            }
            ModelFormat::Chat { .. } => Err(RuntimeError::engine("not a speech recognition model")),
            #[allow(unreachable_patterns)]
            other => Err(unavailable(other.runtime(), &spec.name)),
        }
    }

    #[cfg_attr(not(feature = "llama"), allow(unused_variables))]
    fn load_llm(&self, spec: &ModelSpec, dir: &Path) -> Result<Arc<dyn LlmRuntime>, RuntimeError> {
        match spec.format {
            #[cfg(feature = "llama")]
            ModelFormat::Chat { model, disable_thinking } => {
                Ok(Arc::new(llama::Chat::load(&model_file(dir, model)?, disable_thinking)?))
            }
            #[cfg(not(feature = "llama"))]
            ModelFormat::Chat { .. } => Err(unavailable(spec.format.runtime(), &spec.name)),
            _ => Err(RuntimeError::engine("not a text generation model")),
        }
    }
}

fn unavailable(runtime: Runtime, name: &str) -> RuntimeError {
    RuntimeError::NotCompiled { name: name.to_string(), runtime }
}

/// Resolve the catalog's exact file role, never an arbitrary matching file in the directory.
#[cfg(any(feature = "llama", feature = "sherpa", test))]
fn model_file(dir: &Path, name: &str) -> Result<std::path::PathBuf, RuntimeError> {
    let path = dir.join(name);
    if path.is_file() { Ok(path) } else { Err(RuntimeError::MissingFile(path)) }
}
