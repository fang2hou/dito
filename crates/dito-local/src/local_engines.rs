//! Loading and running local models behind the core `Transcriber` / `Polisher` traits.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use dito_core::config::LocalPolishParams;
use dito_core::error::{ErrorKind, ProviderError, ProviderResult};
use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::provider::{AsrRequest, PolishRequest, Polisher, Transcriber, Transcript};

use crate::catalog::{ModelKind, Sampling};
use crate::manager::ModelManager;
use crate::runtime::{AsrRuntime, LlmRuntime, RuntimeLoader};
use crate::runtime_error::RuntimeError;

#[derive(Clone)]
enum Slot {
    Loading,
    Stopping,
    Asr(Arc<dyn AsrRuntime>),
    Llm(Arc<dyn LlmRuntime>),
    Failed(String),
}

#[derive(Default)]
struct Metrics {
    inflight: AtomicUsize,
    last_ms: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EngineMetrics {
    pub inflight: usize,
    pub last_ms: u64,
}

pub struct LocalEngines {
    manager: ModelManager,
    loader: Arc<dyn RuntimeLoader>,
    slots: Mutex<HashMap<String, Slot>>,
    load_lock: tokio::sync::Mutex<()>,
    metrics: Mutex<HashMap<String, Arc<Metrics>>>,
}

impl LocalEngines {
    pub fn new(manager: ModelManager, loader: Arc<dyn RuntimeLoader>) -> Arc<Self> {
        Arc::new(Self {
            manager,
            loader,
            slots: Mutex::default(),
            load_lock: tokio::sync::Mutex::new(()),
            metrics: Mutex::default(),
        })
    }

    pub fn manager(&self) -> &ModelManager {
        &self.manager
    }

    fn metrics_for(&self, id: &str) -> Arc<Metrics> {
        self.metrics.lock().expect("engine state mutex poisoned").entry(id.to_string()).or_default().clone()
    }

    pub fn metrics(&self, id: &str) -> EngineMetrics {
        let m = self.metrics_for(id);
        EngineMetrics { inflight: m.inflight.load(Ordering::Relaxed), last_ms: m.last_ms.load(Ordering::Relaxed) }
    }

    /// The runtime is reloaded on next use.
    pub fn unload(&self, id: &str) {
        let old = {
            let mut slots = self.slots.lock().expect("engine state mutex poisoned");
            if matches!(slots.get(id), None | Some(Slot::Stopping)) {
                return;
            }
            slots.insert(id.to_string(), Slot::Stopping)
        };
        if matches!(old, Some(Slot::Loading)) {
            // The loader completes the stop after releasing any runtime it produced.
            return;
        }
        // Runtime destruction may wait for Metal. Keep status reads and the UI responsive.
        drop(old);
        let mut slots = self.slots.lock().expect("engine state mutex poisoned");
        if matches!(slots.get(id), Some(Slot::Stopping)) {
            slots.remove(id);
        }
    }

    /// Call before quitting: llama.cpp's Metal backend asserts at process exit if a model is still resident.
    pub fn unload_all(&self) {
        self.slots.lock().expect("engine state mutex poisoned").clear();
    }

    pub async fn restart(&self, id: &str) -> EngineStatus {
        self.unload(id);
        let _ = self.ensure_loaded(id).await;
        self.status(id)
    }

    /// A failure is recorded in the engine status, not returned.
    pub async fn warm(&self, id: &str) {
        let _ = self.ensure_loaded(id).await;
    }

    async fn ensure_loaded(&self, id: &str) -> ProviderResult<Slot> {
        let spec = self
            .manager
            .spec(id)
            .ok_or_else(|| ProviderError::new(ErrorKind::ModelNotFound, format!("unknown local model {id}")))?
            .clone();
        if !self.manager.is_installed(id) {
            return Err(ProviderError::new(ErrorKind::Unavailable, format!("{} is not downloaded", spec.name)));
        }
        let _guard = self.load_lock.lock().await;
        let existing = self.slots.lock().expect("engine state mutex poisoned").get(id).cloned();
        if let Some(slot @ (Slot::Asr(_) | Slot::Llm(_))) = existing {
            return Ok(slot);
        }
        {
            let mut slots = self.slots.lock().expect("engine state mutex poisoned");
            if matches!(slots.get(id), Some(Slot::Stopping)) {
                return Err(ProviderError::new(ErrorKind::Unavailable, "model is stopping"));
            }
            slots.insert(id.to_string(), Slot::Loading);
        }
        let (loader, dir) = (self.loader.clone(), self.manager.model_dir(id));
        let loaded = tokio::task::spawn_blocking(move || match spec.kind {
            ModelKind::Asr => loader.load_asr(&spec, &dir).map(Slot::Asr),
            ModelKind::Polish => loader.load_llm(&spec, &dir).map(Slot::Llm),
        })
        .await
        .map_err(|e| ProviderError::new(ErrorKind::Other, e.to_string()))?;
        let mut slots = self.slots.lock().expect("engine state mutex poisoned");
        if !matches!(slots.get(id), Some(Slot::Loading)) {
            let stopping = matches!(slots.get(id), Some(Slot::Stopping));
            drop(slots);
            drop(loaded);
            if stopping {
                self.slots.lock().expect("engine state mutex poisoned").remove(id);
            }
            return Err(ProviderError::new(ErrorKind::Unavailable, "model loading was stopped"));
        }
        match loaded {
            Ok(slot) => {
                slots.insert(id.to_string(), slot.clone());
                Ok(slot)
            }
            Err(error) => {
                let msg = error.to_string();
                slots.insert(id.to_string(), Slot::Failed(msg.clone()));
                Err(ProviderError::new(ErrorKind::Unavailable, msg))
            }
        }
    }
}

struct LocalAsr {
    rt: Arc<dyn AsrRuntime>,
    metrics: Arc<Metrics>,
}

struct LocalLlm {
    rt: Arc<dyn LlmRuntime>,
    metrics: Arc<Metrics>,
    sampling: Sampling,
}

struct InflightGuard<'a>(&'a Metrics, Instant);
impl<'a> InflightGuard<'a> {
    fn new(m: &'a Metrics) -> Self {
        m.inflight.fetch_add(1, Ordering::Relaxed);
        Self(m, Instant::now())
    }
}
impl Drop for InflightGuard<'_> {
    fn drop(&mut self) {
        self.0.inflight.fetch_sub(1, Ordering::Relaxed);
        self.0.last_ms.store(self.1.elapsed().as_millis() as u64, Ordering::Relaxed);
    }
}

/// Raises the stop flag when the call's future is dropped, so a cancelled dictation frees the model for the next one
/// instead of letting it finish for nobody.
struct StopOnDrop(Arc<AtomicBool>);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Output that used the whole token budget is cut off mid-text and would be typed as if it were complete.
fn runtime_failure(error: RuntimeError) -> ProviderError {
    match error {
        RuntimeError::Truncated => ProviderError::new(ErrorKind::BadResponse, "output was cut off at the token limit"),
        other => ProviderError::new(ErrorKind::Other, other.to_string()),
    }
}

#[async_trait]
impl Transcriber for LocalAsr {
    async fn transcribe(&self, req: &AsrRequest<'_>) -> ProviderResult<Transcript> {
        let _g = InflightGuard::new(&self.metrics);
        let (rt, samples) = (self.rt.clone(), req.clip.samples.clone());
        let (lang, words) = (req.language.map(str::to_string), req.hotwords.to_vec());
        let stop = Arc::new(AtomicBool::new(false));
        let _cancel = StopOnDrop(stop.clone());
        let text = tokio::task::spawn_blocking(move || rt.transcribe(&samples, lang.as_deref(), &words, &stop))
            .await
            .map_err(|e| ProviderError::new(ErrorKind::Other, e.to_string()))?
            .map_err(runtime_failure)?;
        let lang_out = text.language.clone();
        let text = text.text.trim().to_string();
        if text.is_empty() {
            return Err(ProviderError::new(ErrorKind::NoSpeech, "empty transcription"));
        }
        Ok(Transcript { text, language: lang_out })
    }
}

#[async_trait]
impl Polisher for LocalLlm {
    async fn polish(&self, req: &PolishRequest<'_>) -> ProviderResult<String> {
        let _g = InflightGuard::new(&self.metrics);
        let (rt, system, user) = (self.rt.clone(), req.system.to_string(), req.text.to_string());
        let sampling = self.sampling;
        let stop = Arc::new(AtomicBool::new(false));
        let _cancel = StopOnDrop(stop.clone());
        let text = tokio::task::spawn_blocking(move || rt.generate(&system, &user, &sampling, &stop))
            .await
            .map_err(|e| ProviderError::new(ErrorKind::Other, e.to_string()))?
            .map_err(runtime_failure)?;
        let text = text.trim().to_string();
        if text.is_empty() {
            return Err(ProviderError::new(ErrorKind::BadResponse, "empty model output"));
        }
        Ok(text)
    }
}

#[async_trait]
impl LocalBackend for LocalEngines {
    async fn transcriber(&self, model_id: &str) -> ProviderResult<Arc<dyn Transcriber>> {
        match self.ensure_loaded(model_id).await? {
            Slot::Asr(rt) => Ok(Arc::new(LocalAsr { rt, metrics: self.metrics_for(model_id) })),
            _ => Err(ProviderError::new(ErrorKind::Unavailable, format!("{model_id} is not a speech model"))),
        }
    }

    async fn polisher(&self, model_id: &str, params: &LocalPolishParams) -> ProviderResult<Arc<dyn Polisher>> {
        let recommended = self.manager.spec(model_id).and_then(|spec| spec.format.sampling());
        match (self.ensure_loaded(model_id).await?, recommended) {
            (Slot::Llm(rt), Some(recommended)) => Ok(Arc::new(LocalLlm {
                rt,
                metrics: self.metrics_for(model_id),
                sampling: recommended.with_params(params),
            })),
            _ => Err(ProviderError::new(ErrorKind::Unavailable, format!("{model_id} is not a language model"))),
        }
    }

    fn status(&self, model_id: &str) -> EngineStatus {
        if !self.manager.is_installed(model_id) {
            return EngineStatus::NotInstalled;
        }
        match self.slots.lock().expect("engine state mutex poisoned").get(model_id) {
            None => EngineStatus::Stopped,
            Some(Slot::Loading) => EngineStatus::Loading,
            Some(Slot::Stopping) => EngineStatus::Stopping,
            Some(Slot::Asr(_) | Slot::Llm(_)) => EngineStatus::Running,
            Some(Slot::Failed(e)) => EngineStatus::Failed(e.clone()),
        }
    }
}
