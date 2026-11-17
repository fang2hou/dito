//! Loading and running local models behind the core `Transcriber` / `TextModel` traits.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use dito_core::config::LocalTextParams;
use dito_core::error::{ErrorKind, ProviderError, ProviderResult};
use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::intent::Decision;
use dito_core::provider::{
    AsrRequest, ChatCancel, ChatDelta, ChatMessage, ChatRequest, ChatRole, Router, TextModel, TextRequest, Thinking,
    Transcriber, Transcript,
};
use dito_core::trace_log;

use crate::catalog::{ModelKind, Sampling};
use crate::intent;
use crate::manager::ModelManager;
use crate::process::{cpu_time, footprint_bytes};
use crate::runtime::{AsrRuntime, LlmRuntime, RuntimeLoader};
use crate::runtime_error::RuntimeError;

#[derive(Clone)]
enum Slot {
    Loading(Arc<AtomicBool>),
    Stopping(Arc<AtomicBool>),
    Asr(Arc<dyn AsrRuntime>),
    Llm(Arc<dyn LlmRuntime>),
    Failed(String),
}

/// Embedded models are charged the app CPU time spent while they work and its memory gain at load.
/// Process-backed models instead report their worker's kernel figures.
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
    slots: Arc<Mutex<HashMap<String, Slot>>>,
    load_lock: tokio::sync::Mutex<()>,
    usage: Mutex<HashMap<String, Arc<Usage>>>,
}

impl LocalEngines {
    pub fn new(manager: ModelManager, loader: Arc<dyn RuntimeLoader>) -> Arc<Self> {
        Arc::new(Self {
            manager,
            loader,
            slots: Arc::default(),
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
        if let Some(Slot::Asr(runtime)) = self.slots.lock().expect("engine state mutex poisoned").get(id)
            && let Some(pid) = runtime.process_id()
        {
            let usage = crate::process_usage_for(pid);
            return EngineUsage { cpu: usage.cpu, memory_bytes: usage.memory_bytes };
        }
        let usage = self.usage_for(id);
        let since = usage.working_since_ns.load(Ordering::Relaxed);
        let in_flight = if since == 0 { 0 } else { cpu_now_ns().saturating_sub(since) };
        EngineUsage {
            cpu: Duration::from_nanos(usage.cpu_ns.load(Ordering::Relaxed) + in_flight),
            memory_bytes: usage.memory_bytes.load(Ordering::Relaxed),
        }
    }

    pub fn worker_usage(&self) -> crate::ProcessUsage {
        let pids: HashSet<_> = self
            .slots
            .lock()
            .expect("engine state mutex poisoned")
            .values()
            .filter_map(|slot| match slot {
                Slot::Asr(runtime) if runtime.failure().is_none() => runtime.process_id(),
                _ => None,
            })
            .collect();
        pids.into_iter().fold(crate::ProcessUsage::default(), |mut total, pid| {
            let usage = crate::process_usage_for(pid);
            total.cpu = total.cpu.saturating_add(usage.cpu);
            total.memory_bytes = total.memory_bytes.saturating_add(usage.memory_bytes);
            total
        })
    }

    /// The runtime is reloaded on next use.
    pub fn unload(&self, id: &str) {
        let old = {
            let mut slots = self.slots.lock().expect("engine state mutex poisoned");
            if matches!(slots.get(id), None | Some(Slot::Stopping(_))) {
                return;
            }
            let token = match slots.get(id) {
                Some(Slot::Loading(stop)) => stop.clone(),
                _ => Arc::new(AtomicBool::new(true)),
            };
            slots.insert(id.to_string(), Slot::Stopping(token))
        };
        if let Some(Slot::Loading(stop)) = &old {
            stop.store(true, Ordering::Relaxed);
            // The loader completes the stop after releasing any runtime it produced.
            trace_log!(model = id, "model unload requested while it loads, the loader finishes the stop");
            return;
        }
        // Runtime destruction may wait for Metal. Keep status reads and the UI responsive.
        let started = Instant::now();
        if let Some(Slot::Asr(runtime)) = &old {
            runtime.shutdown();
        }
        drop(old);
        let mut slots = self.slots.lock().expect("engine state mutex poisoned");
        if matches!(slots.get(id), Some(Slot::Stopping(_))) {
            slots.remove(id);
        }
        trace_log!(model = id, unload_ms = started.elapsed().as_millis() as u64, "model unloaded");
    }

    /// Call before quitting: llama.cpp's Metal backend asserts at process exit if a model is still resident.
    pub fn unload_all(&self) {
        trace_log!("every model is unloaded");
        let slots = std::mem::take(&mut *self.slots.lock().expect("engine state mutex poisoned"));
        for slot in slots.values() {
            match slot {
                Slot::Loading(stop) => stop.store(true, Ordering::Relaxed),
                Slot::Asr(runtime) => runtime.shutdown(),
                _ => {}
            }
        }
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
        if let Some(Slot::Asr(runtime)) = &existing
            && let Some(error) = runtime.failure()
        {
            return Err(ProviderError::new(ErrorKind::Unavailable, error));
        }
        if let Some(slot @ (Slot::Asr(_) | Slot::Llm(_))) = existing {
            return Ok(slot);
        }
        let stop = Arc::new(AtomicBool::new(false));
        {
            let mut slots = self.slots.lock().expect("engine state mutex poisoned");
            if matches!(slots.get(id), Some(Slot::Stopping(_))) {
                return Err(ProviderError::new(ErrorKind::Unavailable, "model is stopping"));
            }
            slots.insert(id.to_string(), Slot::Loading(stop.clone()));
        }
        let (loader, dir) = (self.loader.clone(), self.manager.model_dir(id));
        let mapped = if spec.kind == ModelKind::Text { mapped_weights_bytes(&dir) } else { 0 };
        let memory_before = footprint_bytes();
        let started = Instant::now();
        trace_log!(model = id, kind = ?spec.kind, "model load started");
        let finished = Arc::new(AtomicBool::new(false));
        let mut cancellation = LoadCancellation {
            slots: self.slots.clone(),
            id: id.to_owned(),
            stop: stop.clone(),
            finished: finished.clone(),
            armed: true,
        };
        let completion = LoadCompletion { slots: self.slots.clone(), id: id.to_owned(), stop: stop.clone(), finished };
        let (loaded, _completion) = tokio::task::spawn_blocking(move || {
            let loaded = match spec.kind {
                ModelKind::Asr => loader.load_asr_cancellable(&spec, &dir, &stop).map(Slot::Asr),
                ModelKind::Text => loader.load_llm(&spec, &dir).map(Slot::Llm),
                ModelKind::Vad => Err(RuntimeError::engine("a voice activity detector is not an engine")),
            };
            if stop.load(Ordering::Relaxed) {
                if let Ok(Slot::Asr(runtime)) = &loaded {
                    runtime.shutdown();
                }
                drop(loaded);
                return (Err(RuntimeError::Cancelled), completion);
            }
            (loaded, completion)
        })
        .await
        .map_err(|e| ProviderError::new(ErrorKind::Other, e.to_string()))?;
        cancellation.armed = false;
        let load_ms = started.elapsed().as_millis() as u64;
        let mut slots = self.slots.lock().expect("engine state mutex poisoned");
        if !matches!(slots.get(id), Some(Slot::Loading(_))) {
            let stopping = matches!(slots.get(id), Some(Slot::Stopping(_)));
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
                let gained = footprint_bytes().saturating_sub(memory_before).saturating_add(mapped);
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

/// A text model's weights are memory-mapped, so they are resident but not part of the footprint that `gained` measures.
fn mapped_weights_bytes(dir: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "gguf"))
        .filter_map(|entry| entry.metadata().ok())
        .map(|meta| meta.len())
        .sum()
}

// Dropping the caller must cancel startup too; otherwise its abandoned slot stays Loading forever.
struct LoadCancellation {
    slots: Arc<Mutex<HashMap<String, Slot>>>,
    id: String,
    stop: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    armed: bool,
}

impl Drop for LoadCancellation {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.stop.store(true, Ordering::Relaxed);
        let mut slots = self.slots.lock().expect("engine state mutex poisoned");
        if matches!(slots.get(&self.id), Some(Slot::Loading(stop)) if Arc::ptr_eq(stop, &self.stop)) {
            if self.finished.load(Ordering::Acquire) {
                slots.remove(&self.id);
            } else {
                slots.insert(self.id.clone(), Slot::Stopping(self.stop.clone()));
            }
        }
    }
}

struct LoadCompletion {
    slots: Arc<Mutex<HashMap<String, Slot>>>,
    id: String,
    stop: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
}

impl Drop for LoadCompletion {
    fn drop(&mut self) {
        self.finished.store(true, Ordering::Release);
        if !self.stop.load(Ordering::Relaxed) {
            return;
        }
        let mut slots = self.slots.lock().expect("engine state mutex poisoned");
        if matches!(slots.get(&self.id), Some(Slot::Stopping(stop)) if Arc::ptr_eq(stop, &self.stop)) {
            slots.remove(&self.id);
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
    /// The catalog id picks the wording of a thinking level.
    model_id: String,
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

const CANCEL_POLL: Duration = Duration::from_millis(25);

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

    /// The runtime decodes on a blocking thread and sends each piece over a channel, so `on_delta` runs here. The
    /// cancel flag is copied to the runtime's stop flag on a short tick, which also reaches a call that is still waiting
    /// for the model or reading the prompt.
    async fn chat(&self, req: &ChatRequest, on_delta: ChatDelta<'_>, cancel: &ChatCancel) -> ProviderResult<String> {
        if cancel.is_cancelled() {
            return Ok(String::new());
        }
        let _working = Working::new(&self.usage);
        let sampling = Sampling {
            temperature: req.temperature.unwrap_or(self.sampling.temperature),
            top_p: req.top_p.or(self.sampling.top_p),
            max_tokens: req.max_tokens.or(self.sampling.max_tokens),
            ..self.sampling
        };
        let (rt, mut messages) = (self.rt.clone(), req.messages.clone());
        let think = req.thinking.as_ref().map(|thinking| *thinking != Thinking::Off);
        if let Some(Thinking::Level(level)) = &req.thinking
            && let Some(instruction) = crate::catalog::reasoning_instruction(&self.model_id, level)
        {
            match messages.iter_mut().find(|m| m.role == ChatRole::System) {
                Some(system) => system.content = format!("{instruction}\n\n{}", system.content),
                None => messages.insert(0, ChatMessage::new(ChatRole::System, instruction)),
            }
        }
        let stop = Arc::new(AtomicBool::new(false));
        let _cancel = StopOnDrop(stop.clone());
        let (pieces, mut received) = tokio::sync::mpsc::unbounded_channel::<String>();
        let blocking_stop = stop.clone();
        let task = tokio::task::spawn_blocking(move || {
            rt.chat(
                &messages,
                &sampling,
                think,
                &mut |piece| {
                    let _ = pieces.send(piece.to_string());
                },
                &blocking_stop,
            )
        });
        let mut tick = tokio::time::interval(CANCEL_POLL);
        let mut reply = String::new();
        loop {
            tokio::select! {
                piece = received.recv() => {
                    let Some(piece) = piece else { break };
                    on_delta(&piece);
                    reply.push_str(&piece);
                }
                _ = tick.tick() => {
                    if cancel.is_cancelled() {
                        stop.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
        match task.await.map_err(|e| ProviderError::new(ErrorKind::Other, e.to_string()))? {
            Ok(_) | Err(RuntimeError::Cancelled) => {}
            Err(error) => return Err(runtime_failure(error)),
        }
        if reply.is_empty() && !cancel.is_cancelled() {
            return Err(ProviderError::new(ErrorKind::BadResponse, "empty model output"));
        }
        Ok(reply)
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
                model_id: model_id.to_string(),
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
            Some(Slot::Loading(_)) => EngineStatus::Loading,
            Some(Slot::Stopping(_)) => EngineStatus::Stopping,
            Some(Slot::Asr(runtime)) => runtime.failure().map_or(EngineStatus::Running, EngineStatus::Failed),
            Some(Slot::Llm(_)) => EngineStatus::Running,
            Some(Slot::Failed(e)) => EngineStatus::Failed(e.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mapped_weights_bytes;

    #[test]
    fn mapped_weights_count_only_gguf_files() {
        let dir = std::env::temp_dir().join(format!("dito-mapped-weights-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("model.gguf"), [0u8; 100]).unwrap();
        std::fs::write(dir.join("other.gguf"), [0u8; 50]).unwrap();
        std::fs::write(dir.join(".download.lock"), []).unwrap();
        assert_eq!(mapped_weights_bytes(&dir), 150);
        assert_eq!(mapped_weights_bytes(&dir.join("missing")), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
