//! Inference runtimes for the on-device models.

use std::path::Path;
use std::sync::Arc;

use dito_local::catalog::{ModelFormat, ModelSpec, Runtime};
use dito_local::{AsrRuntime, LlmRuntime, RuntimeError, RuntimeLoader};

#[cfg(any(feature = "sherpa", feature = "transcribe", test))]
#[cfg_attr(not(any(feature = "sherpa", feature = "transcribe")), allow(dead_code))]
mod chunking;
#[cfg(feature = "llama")]
mod llama;
#[cfg(feature = "nemo")]
mod nemo;
#[cfg(feature = "prism")]
mod prism;
#[cfg(any(feature = "llama", feature = "prism", test))]
#[cfg_attr(not(feature = "llama"), allow(dead_code))]
mod qwen_text;
#[cfg(feature = "llama")]
mod readout;
#[cfg(feature = "sherpa")]
mod sherpa;
#[cfg(feature = "transcribe")]
mod transcribe;
#[cfg(any(feature = "nemo", feature = "prism"))]
mod worker;

#[cfg(feature = "sherpa")]
mod vad;

#[cfg(feature = "sherpa")]
pub use vad::SileroDetectors;

#[derive(Default)]
pub struct Runtimes;

impl RuntimeLoader for Runtimes {
    #[cfg_attr(
        not(any(feature = "llama", feature = "sherpa", feature = "transcribe", feature = "nemo")),
        allow(unused_variables)
    )]
    fn load_asr(&self, spec: &ModelSpec, dir: &Path) -> Result<Arc<dyn AsrRuntime>, RuntimeError> {
        match spec.format {
            #[cfg(feature = "nemo")]
            ModelFormat::NemoSpeech { model, encoder, archive } => Ok(Arc::new(nemo::Speech::load(
                dir,
                model,
                encoder,
                archive,
                &std::sync::atomic::AtomicBool::new(false),
            )?)),
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
            #[cfg(feature = "transcribe")]
            ModelFormat::TranscribeCpp { model, .. } => {
                Ok(Arc::new(transcribe::Speech::load(&model_file(dir, model)?)?))
            }
            #[cfg(feature = "llama")]
            ModelFormat::QwenAsr { model, projector } => {
                Ok(Arc::new(llama::Asr::load(&model_file(dir, model)?, &model_file(dir, projector)?)?))
            }
            ModelFormat::Chat { .. } | ModelFormat::PrismChat { .. } => {
                Err(RuntimeError::engine("not a speech recognition model"))
            }
            #[allow(unreachable_patterns)]
            other => Err(unavailable(other.runtime(), &spec.name)),
        }
    }

    fn load_asr_cancellable(
        &self,
        spec: &ModelSpec,
        dir: &Path,
        stop: &std::sync::atomic::AtomicBool,
    ) -> Result<Arc<dyn AsrRuntime>, RuntimeError> {
        #[cfg(feature = "nemo")]
        if let ModelFormat::NemoSpeech { model, encoder, archive } = spec.format {
            return Ok(Arc::new(nemo::Speech::load(dir, model, encoder, archive, stop)?));
        }
        let _ = stop;
        self.load_asr(spec, dir)
    }

    #[cfg_attr(not(any(feature = "llama", feature = "prism")), allow(unused_variables))]
    fn load_llm(&self, spec: &ModelSpec, dir: &Path) -> Result<Arc<dyn LlmRuntime>, RuntimeError> {
        self.load_llm_cancellable(spec, dir, &std::sync::atomic::AtomicBool::new(false))
    }

    #[cfg_attr(not(any(feature = "llama", feature = "prism")), allow(unused_variables))]
    fn load_llm_cancellable(
        &self,
        spec: &ModelSpec,
        dir: &Path,
        stop: &std::sync::atomic::AtomicBool,
    ) -> Result<Arc<dyn LlmRuntime>, RuntimeError> {
        match spec.format {
            #[cfg(feature = "llama")]
            ModelFormat::Chat { model, disable_thinking, .. } => {
                Ok(Arc::new(llama::Chat::load(&model_file(dir, model)?, disable_thinking)?))
            }
            #[cfg(not(feature = "llama"))]
            ModelFormat::Chat { .. } => Err(unavailable(spec.format.runtime(), &spec.name)),
            #[cfg(feature = "prism")]
            ModelFormat::PrismChat { model, archive, .. } => {
                Ok(Arc::new(prism::Chat::load(dir, model, archive, stop)?))
            }
            #[cfg(not(feature = "prism"))]
            ModelFormat::PrismChat { .. } => Err(unavailable(spec.format.runtime(), &spec.name)),
            _ => Err(RuntimeError::engine("not a text generation model")),
        }
    }
}

fn unavailable(runtime: Runtime, name: &str) -> RuntimeError {
    RuntimeError::NotCompiled { name: name.to_string(), runtime }
}

/// Never an arbitrary matching file in the directory: the catalog names the exact file.
#[cfg(any(feature = "llama", feature = "sherpa", feature = "nemo", feature = "prism", feature = "transcribe", test))]
fn model_file(dir: &Path, name: &str) -> Result<std::path::PathBuf, RuntimeError> {
    let path = dir.join(name);
    if path.is_file() { Ok(path) } else { Err(RuntimeError::MissingFile(path)) }
}
