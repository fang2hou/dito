//! Loading and running local models behind the core `Transcriber` / `TextModel` traits.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use dito_core::config::LocalTextParams;
use dito_core::error::{ErrorKind, ProviderError, ProviderResult};
use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::intent::Decision;
use dito_core::provider::{AsrRequest, Router, TextModel, TextRequest, Transcriber, Transcript};
use dito_core::trace_log;

use crate::catalog::{ModelKind, Sampling};
use crate::intent;
use crate::manager::ModelManager;
use crate::process::{cpu_time, footprint_bytes};
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

/// Models run inside the app's process, so the kernel reports no figures of their own: a model is charged the
/// process CPU time spent while it works and the memory the process gained while it loaded.
#[derive(Default)]
struct Usage {
    cpu_ns: AtomicU64,
    /// Process CPU time when the call in flight began; 0 while idle.
    working_since_ns: AtomicU64,
    memory_bytes: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EngineUsage {
    /// Cumulative, including the call in flight.
    pub cpu: Duration,
    pub memory_bytes: u64,
}

fn cpu_now_ns() -> u64 {
    cpu_time().as_nanos() as u64
}

pub struct LocalEngines {
    manager: ModelManager,
    loader: Arc<dyn RuntimeLoader>,
    slots: Mutex<HashMap<String, Slot>>,
    load_lock: tokio::sync::Mutex<()>,
    usage: Mutex<HashMap<String, Arc<Usage>>>,
}

impl LocalEngines {
    pub fn new(manager: ModelManager, loader: Arc<dyn RuntimeLoader>) -> Arc<Self> {
        Arc::new(Self {
            manager,
            loader,
            slots: Mutex::default(),
            load_lock: tokio::sync::Mutex::new(()),
            usage: Mutex::default(),
        })
    }

    pub fn manager(&self) -> &ModelManager {
        &self.manager
    }

    fn usage_for(&self, id: &str) -> Arc<Usage> {
        self.usage.lock().expect("engine state mutex poisoned").entry(id.to_string()).or_default().clone()
    }

    pub fn usage(&self, id: &str) -> EngineUsage {
        let usage = self.usage_for(id);
        let since = usage.working_since_ns.load(Ordering::Relaxed);
        let in_flight = if since == 0 { 0 } else { cpu_now_ns().saturating_sub(since) };
        EngineUsage {
            cpu: Duration::from_nanos(usage.cpu_ns.load(Ordering::Relaxed) + in_flight),
            memory_bytes: usage.memory_bytes.load(Ordering::Relaxed),
        }
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
            trace_log!(model = id, "model unload requested while it loads, the loader finishes the stop");
            return;
        }
        // Runtime destruction may wait for Metal. Keep status reads and the UI responsive.
        let started = Instant::now();
        drop(old);
        let mut slots = self.slots.lock().expect("engine state mutex poisoned");
        if matches!(slots.get(id), Some(Slot::Stopping)) {
            slots.remove(id);
        }
        trace_log!(model = id, unload_ms = started.elapsed().as_millis() as u64, "model unloaded");
    }

    /// Call before quitting: llama.cpp's Metal backend asserts at process exit if a model is still resident.
    pub fn unload_all(&self) {
        trace_log!("every model is unloaded");
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
        let memory_before = footprint_bytes();
        let started = Instant::now();
        trace_log!(model = id, kind = ?spec.kind, "model load started");
        let loaded = tokio::task::spawn_blocking(move || match spec.kind {
            ModelKind::Asr => loader.load_asr(&spec, &dir).map(Slot::Asr),
            ModelKind::Text => loader.load_llm(&spec, &dir).map(Slot::Llm),
            ModelKind::Vad => Err(RuntimeError::engine("a voice activity detector is not an engine")),
        })
        .await
        .map_err(|e| ProviderError::new(ErrorKind::Other, e.to_string()))?;
        let load_ms = started.elapsed().as_millis() as u64;
        let mut slots = self.slots.lock().expect("engine state mutex poisoned");
        if !matches!(slots.get(id), Some(Slot::Loading)) {
            let stopping = matches!(slots.get(id), Some(Slot::Stopping));
            trace_log!(model = id, load_ms, stopping, "model load discarded, it was stopped meanwhile");
            drop(slots);
            drop(loaded);
            if stopping {
                self.slots.lock().expect("engine state mutex poisoned").remove(id);
            }
            return Err(ProviderError::new(ErrorKind::Unavailable, "model loading was stopped"));
        }
        match loaded {
            Ok(slot) => {
                let gained = footprint_bytes().saturating_sub(memory_before);
                trace_log!(model = id, load_ms, memory_bytes = gained, "model loaded");
                self.usage_for(id).memory_bytes.store(gained, Ordering::Relaxed);
                slots.insert(id.to_string(), slot.clone());
                Ok(slot)
            }
            Err(error) => {
                let msg = error.to_string();
                trace_log!(model = id, load_ms, error = %msg, "model load failed");
                slots.insert(id.to_string(), Slot::Failed(msg.clone()));
                Err(ProviderError::new(ErrorKind::Unavailable, msg))
            }
        }
    }
}

struct LocalAsr {
    rt: Arc<dyn AsrRuntime>,
    usage: Arc<Usage>,
}

struct LocalLlm {
    rt: Arc<dyn LlmRuntime>,
    usage: Arc<Usage>,
    sampling: Sampling,
}

struct LocalRouter {
    rt: Arc<dyn LlmRuntime>,
    usage: Arc<Usage>,
    /// The catalog id picks the heads bundled for this model file.
    model_id: String,
}

struct Working<'a>(&'a Usage);
impl<'a> Working<'a> {
    fn new(usage: &'a Usage) -> Self {
        usage.working_since_ns.store(cpu_now_ns().max(1), Ordering::Relaxed);
        Self(usage)
    }
}
impl Drop for Working<'_> {
    fn drop(&mut self) {
        let since = self.0.working_since_ns.swap(0, Ordering::Relaxed);
        if since != 0 {
            self.0.cpu_ns.fetch_add(cpu_now_ns().saturating_sub(since), Ordering::Relaxed);
        }
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
        let _working = Working::new(&self.usage);
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
impl TextModel for LocalLlm {
    async fn polish(&self, req: &TextRequest<'_>) -> ProviderResult<String> {
        let _working = Working::new(&self.usage);
        let (rt, system, user) = (self.rt.clone(), req.system.to_string(), req.text.to_string());
        let examples = req.examples;
        let sampling = self.sampling;
        let stop = Arc::new(AtomicBool::new(false));
        let _cancel = StopOnDrop(stop.clone());
        let text = tokio::task::spawn_blocking(move || rt.generate(&system, examples, &user, &sampling, &stop))
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
impl Router for LocalRouter {
    async fn decide(&self, instruction: &str) -> ProviderResult<Decision> {
        let _working = Working::new(&self.usage);
        let (rt, model_id, instruction) = (self.rt.clone(), self.model_id.clone(), instruction.to_string());
        let stop = Arc::new(AtomicBool::new(false));
        let _cancel = StopOnDrop(stop.clone());
        // The run's span goes along, so the log line of the blocking pass carries the run id.
        let span = tracing::Span::current();
        tokio::task::spawn_blocking(move || {
            span.in_scope(|| intent::decide(rt.as_ref(), &model_id, &instruction, &stop))
        })
        .await
        .map_err(|e| ProviderError::new(ErrorKind::Other, e.to_string()))?
        .map_err(runtime_failure)
    }
}

#[async_trait]
impl LocalBackend for LocalEngines {
    async fn transcriber(&self, model_id: &str) -> ProviderResult<Arc<dyn Transcriber>> {
        match self.ensure_loaded(model_id).await? {
            Slot::Asr(rt) => Ok(Arc::new(LocalAsr { rt, usage: self.usage_for(model_id) })),
            _ => Err(ProviderError::new(ErrorKind::Unavailable, format!("{model_id} is not a speech model"))),
        }
    }

    async fn text_model(&self, model_id: &str, params: &LocalTextParams) -> ProviderResult<Arc<dyn TextModel>> {
        let recommended = self.manager.spec(model_id).and_then(|spec| spec.format.sampling());
        match (self.ensure_loaded(model_id).await?, recommended) {
            (Slot::Llm(rt), Some(recommended)) => Ok(Arc::new(LocalLlm {
                rt,
                usage: self.usage_for(model_id),
                sampling: recommended.with_params(params),
            })),
            _ => Err(ProviderError::new(ErrorKind::Unavailable, format!("{model_id} is not a language model"))),
        }
    }

    async fn router(&self, model_id: &str) -> ProviderResult<Arc<dyn Router>> {
        match self.ensure_loaded(model_id).await? {
            Slot::Llm(rt) => {
                Ok(Arc::new(LocalRouter { rt, usage: self.usage_for(model_id), model_id: model_id.to_string() }))
            }
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
