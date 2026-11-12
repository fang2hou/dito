//! LocalEngines with a fake runtime.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use dito_core::audio::AudioClip;
use dito_core::config::{ConfigStore, LocalPolishParams};
use dito_core::error::ErrorKind;
use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::intent::{Intent, Reading, Request};
use dito_core::provider::{AsrRequest, Example, PolishRequest};
use dito_local::catalog::{FileSpec, ModelFormat, ModelKind, ModelSpec, Sampling};
use dito_local::{
    AsrOutput, AsrRuntime, LlmRuntime, LocalEngines, ModelManager, NoRuntime, RuntimeError, RuntimeLoader,
};

const RECOMMENDED: Sampling = Sampling { temperature: 0.2, top_p: Some(0.9), top_k: None, max_tokens: None };

const NO_PARAMS: LocalPolishParams =
    LocalPolishParams { temperature: None, top_p: None, top_k: None, max_tokens: None };

struct Fake {
    loads: AtomicUsize,
    fail: bool,
}

struct FakeAsr;
impl AsrRuntime for FakeAsr {
    fn transcribe(
        &self,
        samples: &[f32],
        _: Option<&str>,
        _: &[String],
        _: &AtomicBool,
    ) -> Result<AsrOutput, RuntimeError> {
        Ok(AsrOutput { text: format!("{} samples", samples.len()), language: Some("ja".into()) })
    }
}
struct FakeLlm;
impl LlmRuntime for FakeLlm {
    fn generate(
        &self,
        _: &str,
        _: &[Example],
        user: &str,
        _: &Sampling,
        _: &AtomicBool,
    ) -> Result<String, RuntimeError> {
        Ok(user.to_uppercase())
    }
}

impl RuntimeLoader for Fake {
    fn load_asr(&self, _: &ModelSpec, _: &Path) -> Result<Arc<dyn AsrRuntime>, RuntimeError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        if self.fail { Err(RuntimeError::engine("out of memory")) } else { Ok(Arc::new(FakeAsr)) }
    }
    fn load_llm(&self, _: &ModelSpec, _: &Path) -> Result<Arc<dyn LlmRuntime>, RuntimeError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(FakeLlm))
    }
}

fn installed_manager() -> (ModelManager, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = Arc::new(ConfigStore::open(dir.path().join("c.toml")).unwrap());
    cfg.update(|c| c.models_dir = Some(dir.path().join("models"))).unwrap();
    let mk = |id: &str, kind| ModelSpec {
        id: id.into(),
        name: id.into(),
        kind,
        files: vec![FileSpec { name: "f".into(), url: String::new(), size: 3, sha256: None }],
        memory_mb: 1,
        format: match kind {
            ModelKind::Asr => ModelFormat::SenseVoice { model: "f", tokens: "f" },
            ModelKind::Polish => ModelFormat::Chat { model: "f", disable_thinking: false, sampling: RECOMMENDED },
        },
        language_focus: None,
    };
    let mgr = ModelManager::with_catalog(
        vec![mk("asr", ModelKind::Asr), mk("llm", ModelKind::Polish), mk("missing", ModelKind::Asr)],
        cfg,
    );
    for id in ["asr", "llm"] {
        std::fs::create_dir_all(mgr.model_dir(id)).unwrap();
        std::fs::write(mgr.model_dir(id).join("f"), b"abc").unwrap();
    }
    mgr.refresh();
    (mgr, dir)
}

#[tokio::test]
async fn loads_lazily_once_and_runs_inference_off_the_async_thread() {
    let (mgr, _d) = installed_manager();
    let fake = Arc::new(Fake { loads: AtomicUsize::new(0), fail: false });
    let eng = LocalEngines::new(mgr, fake.clone());
    assert_eq!(eng.status("asr"), EngineStatus::Stopped);

    let clip = AudioClip::new(vec![0.1; 1600]);
    for _ in 0..2 {
        let t = eng.transcriber("asr").await.unwrap();
        let out = t.transcribe(&AsrRequest { clip: &clip, language: None, hotwords: &[] }).await.unwrap();
        assert_eq!(out.text, "1600 samples");
        assert_eq!(out.language.as_deref(), Some("ja"));
    }
    assert_eq!(fake.loads.load(Ordering::SeqCst), 1, "second call reuses the loaded runtime");
    assert_eq!(eng.status("asr"), EngineStatus::Running);

    let p = eng.polisher("llm", &NO_PARAMS).await.unwrap();
    assert_eq!(p.polish(&PolishRequest { system: "s", examples: &[], text: "hi" }).await.unwrap(), "HI");
}

struct Reader;
impl LlmRuntime for Reader {
    fn generate(&self, _: &str, _: &[Example], _: &str, _: &Sampling, _: &AtomicBool) -> Result<String, RuntimeError> {
        Err(RuntimeError::engine("a router never generates"))
    }
    fn read(&self, request: &Request, _: &AtomicBool) -> Result<Reading, RuntimeError> {
        assert!(request.user.contains("translate it"), "the instruction reaches the model");
        Ok(Reading { option_logits: vec![0., 0., 8., 0., 0., 0., 0., 0.], features: Vec::new() })
    }
}

struct ReaderLoader;
impl RuntimeLoader for ReaderLoader {
    fn load_asr(&self, _: &ModelSpec, _: &Path) -> Result<Arc<dyn AsrRuntime>, RuntimeError> {
        Err(RuntimeError::engine("no speech here"))
    }
    fn load_llm(&self, _: &ModelSpec, _: &Path) -> Result<Arc<dyn LlmRuntime>, RuntimeError> {
        Ok(Arc::new(Reader))
    }
}

#[tokio::test]
async fn the_router_decides_on_the_loaded_chat_model_and_fails_without_one() {
    let (mgr, _d) = installed_manager();
    let eng = LocalEngines::new(mgr, Arc::new(ReaderLoader));
    let decision = eng.router("llm").await.unwrap().decide("translate it").await.unwrap();
    assert_eq!(decision.intent, Intent::Translate);
    assert!(decision.is_confident());
    assert_eq!(eng.router("asr").await.err().unwrap().kind, ErrorKind::Unavailable);
}

struct Probe(Arc<std::sync::Mutex<Option<Sampling>>>);
impl LlmRuntime for Probe {
    fn generate(
        &self,
        _: &str,
        _: &[Example],
        user: &str,
        sampling: &Sampling,
        _: &AtomicBool,
    ) -> Result<String, RuntimeError> {
        *self.0.lock().unwrap() = Some(*sampling);
        Ok(user.into())
    }
}
struct ProbeLoader(Arc<std::sync::Mutex<Option<Sampling>>>);
impl RuntimeLoader for ProbeLoader {
    fn load_asr(&self, _: &ModelSpec, _: &Path) -> Result<Arc<dyn AsrRuntime>, RuntimeError> {
        Err(RuntimeError::engine("speech is not under test"))
    }
    fn load_llm(&self, _: &ModelSpec, _: &Path) -> Result<Arc<dyn LlmRuntime>, RuntimeError> {
        Ok(Arc::new(Probe(self.0.clone())))
    }
}

#[tokio::test]
async fn a_polisher_samples_with_the_recommendation_under_the_parameters_of_its_model() {
    let (manager, _dir) = installed_manager();
    let seen = Arc::new(std::sync::Mutex::new(None));
    let engine = LocalEngines::new(manager, Arc::new(ProbeLoader(seen.clone())));
    let request = PolishRequest { system: "s", examples: &[], text: "hi" };

    engine.polisher("llm", &NO_PARAMS).await.unwrap().polish(&request).await.unwrap();
    assert_eq!(*seen.lock().unwrap(), Some(RECOMMENDED), "nothing set: the catalog's recommendation");

    let params = LocalPolishParams { temperature: Some(0.7), top_k: Some(40), max_tokens: Some(64), ..NO_PARAMS };
    engine.polisher("llm", &params).await.unwrap().polish(&request).await.unwrap();
    let expected = Sampling { temperature: 0.7, top_p: Some(0.9), top_k: Some(40), max_tokens: Some(64) };
    assert_eq!(*seen.lock().unwrap(), Some(expected), "what is set wins, what is not keeps its recommendation");

    engine.polisher("llm", &NO_PARAMS).await.unwrap().polish(&request).await.unwrap();
    assert_eq!(*seen.lock().unwrap(), Some(RECOMMENDED), "parameters belong to one polisher, not to the loaded model");
}

#[tokio::test]
async fn restart_reloads_the_runtime() {
    let (mgr, _d) = installed_manager();
    let fake = Arc::new(Fake { loads: AtomicUsize::new(0), fail: false });
    let eng = LocalEngines::new(mgr, fake.clone());
    eng.transcriber("asr").await.unwrap();
    assert_eq!(eng.restart("asr").await, EngineStatus::Running);
    assert_eq!(fake.loads.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn not_installed_and_load_failures_are_reported_with_reasons() {
    let (mgr, _d) = installed_manager();
    let eng = LocalEngines::new(mgr, Arc::new(Fake { loads: AtomicUsize::new(0), fail: true }));
    assert_eq!(eng.status("missing"), EngineStatus::NotInstalled);
    assert_eq!(eng.transcriber("missing").await.err().unwrap().kind, ErrorKind::Unavailable);
    let err = eng.transcriber("asr").await.err().unwrap();
    assert_eq!(err.kind, ErrorKind::Unavailable);
    assert_eq!(eng.status("asr"), EngineStatus::Failed("out of memory".into()));
}

#[tokio::test]
async fn without_a_runtime_the_status_is_failed_never_a_fake_success() {
    let (mgr, _d) = installed_manager();
    let eng = LocalEngines::new(mgr, Arc::new(NoRuntime));
    assert!(eng.transcriber("asr").await.is_err());
    assert!(matches!(eng.status("asr"), EngineStatus::Failed(m) if m.contains("no inference runtime")));
}

#[derive(Clone)]
struct ControlledLoader {
    entered: std::sync::mpsc::Sender<()>,
    release: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
    block_drop: bool,
}
struct ControlledAsr {
    gate: Arc<ControlledLoader>,
}
impl AsrRuntime for ControlledAsr {
    fn transcribe(&self, _: &[f32], _: Option<&str>, _: &[String], _: &AtomicBool) -> Result<AsrOutput, RuntimeError> {
        Ok(AsrOutput { text: "test".into(), language: None })
    }
}
impl Drop for ControlledAsr {
    fn drop(&mut self) {
        self.gate.entered.send(()).unwrap();
        self.gate.release.lock().unwrap().recv().unwrap();
    }
}
impl RuntimeLoader for ControlledLoader {
    fn load_asr(&self, _: &ModelSpec, _: &Path) -> Result<Arc<dyn AsrRuntime>, RuntimeError> {
        if self.block_drop {
            Ok(Arc::new(ControlledAsr { gate: Arc::new(self.clone()) }))
        } else {
            self.entered.send(()).unwrap();
            self.release.lock().unwrap().recv().unwrap();
            Ok(Arc::new(FakeAsr))
        }
    }
    fn load_llm(&self, _: &ModelSpec, _: &Path) -> Result<Arc<dyn LlmRuntime>, RuntimeError> {
        Ok(Arc::new(FakeLlm))
    }
}
fn controlled_engine(
    block_drop: bool,
) -> (Arc<LocalEngines>, tempfile::TempDir, std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
    let (manager, dir) = installed_manager();
    let (entered, observe) = std::sync::mpsc::channel();
    let (release, gate) = std::sync::mpsc::channel();
    let loader = Arc::new(ControlledLoader { entered, release: Arc::new(std::sync::Mutex::new(gate)), block_drop });
    (LocalEngines::new(manager, loader), dir, observe, release)
}

#[tokio::test]
async fn runtime_release_is_observable_as_stopping_without_holding_the_status_lock() {
    let (engine, _dir, entered, release) = controlled_engine(true);
    engine.warm("asr").await;
    let retiring = engine.clone();
    let task = tokio::task::spawn_blocking(move || retiring.unload("asr"));
    entered.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    assert_eq!(engine.status("asr"), EngineStatus::Stopping);
    release.send(()).unwrap();
    task.await.unwrap();
    assert_eq!(engine.status("asr"), EngineStatus::Stopped);
}

#[tokio::test]
async fn stopping_during_load_does_not_publish_the_finished_runtime() {
    let (engine, _dir, entered, release) = controlled_engine(false);
    let loading = engine.clone();
    let task = tokio::spawn(async move { loading.warm("asr").await });
    // Let the async task reach its blocking loader before waiting on its signal.
    tokio::task::yield_now().await;
    entered.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    engine.unload("asr");
    assert_eq!(engine.status("asr"), EngineStatus::Stopping);
    release.send(()).unwrap();
    task.await.unwrap();
    assert_eq!(engine.status("asr"), EngineStatus::Stopped);
}

const WAIT: Duration = Duration::from_secs(5);

/// One runtime serving as both the speech and the language model, so a test decides how every inference call ends.
struct Shared<R>(Arc<R>);
impl<R: AsrRuntime + LlmRuntime + 'static> RuntimeLoader for Shared<R> {
    fn load_asr(&self, _: &ModelSpec, _: &Path) -> Result<Arc<dyn AsrRuntime>, RuntimeError> {
        Ok(self.0.clone())
    }
    fn load_llm(&self, _: &ModelSpec, _: &Path) -> Result<Arc<dyn LlmRuntime>, RuntimeError> {
        Ok(self.0.clone())
    }
}
fn engine_with<R: AsrRuntime + LlmRuntime + 'static>(runtime: R) -> (Arc<LocalEngines>, tempfile::TempDir) {
    let (manager, dir) = installed_manager();
    (LocalEngines::new(manager, Arc::new(Shared(Arc::new(runtime)))), dir)
}

/// Keeps decoding until the stop flag is raised, like a model still generating when its dictation is cancelled.
struct Spinning(std::sync::mpsc::Sender<&'static str>);
impl Spinning {
    fn spin(&self, stop: &AtomicBool) -> RuntimeError {
        let _ = self.0.send("started");
        // Bounded, so a flag that never arrives fails the test instead of hanging the Tokio runtime's shutdown.
        for _ in 0..10_000 {
            if stop.load(Ordering::Relaxed) {
                let _ = self.0.send("saw stop");
                return RuntimeError::Cancelled;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        RuntimeError::engine("the stop flag was never raised")
    }
}
impl AsrRuntime for Spinning {
    fn transcribe(
        &self,
        _: &[f32],
        _: Option<&str>,
        _: &[String],
        stop: &AtomicBool,
    ) -> Result<AsrOutput, RuntimeError> {
        Err(self.spin(stop))
    }
}
impl LlmRuntime for Spinning {
    fn generate(
        &self,
        _: &str,
        _: &[Example],
        _: &str,
        _: &Sampling,
        stop: &AtomicBool,
    ) -> Result<String, RuntimeError> {
        Err(self.spin(stop))
    }
}

async fn abort_and_expect_stop<T>(call: tokio::task::JoinHandle<T>, events: &std::sync::mpsc::Receiver<&'static str>) {
    // Let the task reach its blocking call before waiting on the runtime's signal.
    tokio::task::yield_now().await;
    assert_eq!(events.recv_timeout(WAIT), Ok("started"));
    call.abort();
    assert!(call.await.is_err_and(|e| e.is_cancelled()), "the call finished instead of being dropped");
    assert_eq!(events.recv_timeout(WAIT), Ok("saw stop"), "the runtime kept generating for a dropped call");
}

#[tokio::test]
async fn dropping_a_polish_call_raises_the_stop_flag_for_the_runtime() {
    let (events, observed) = std::sync::mpsc::channel();
    let (engine, _dir) = engine_with(Spinning(events));
    let polisher = engine.polisher("llm", &NO_PARAMS).await.unwrap();
    let call =
        tokio::spawn(async move { polisher.polish(&PolishRequest { system: "s", examples: &[], text: "hi" }).await });
    abort_and_expect_stop(call, &observed).await;
}

#[tokio::test]
async fn dropping_a_transcription_call_raises_the_stop_flag_for_the_runtime() {
    let (events, observed) = std::sync::mpsc::channel();
    let (engine, _dir) = engine_with(Spinning(events));
    let transcriber = engine.transcriber("asr").await.unwrap();
    let call = tokio::spawn(async move {
        let clip = AudioClip::new(vec![0.1; 1600]);
        transcriber.transcribe(&AsrRequest { clip: &clip, language: None, hotwords: &[] }).await
    });
    abort_and_expect_stop(call, &observed).await;
}

struct CutOff;
impl AsrRuntime for CutOff {
    fn transcribe(&self, _: &[f32], _: Option<&str>, _: &[String], _: &AtomicBool) -> Result<AsrOutput, RuntimeError> {
        Err(RuntimeError::Truncated)
    }
}
impl LlmRuntime for CutOff {
    fn generate(&self, _: &str, _: &[Example], _: &str, _: &Sampling, _: &AtomicBool) -> Result<String, RuntimeError> {
        Err(RuntimeError::Truncated)
    }
}

#[tokio::test]
async fn output_cut_off_at_the_token_limit_is_a_bad_response_not_a_result() {
    let (engine, _dir) = engine_with(CutOff);
    let polisher = engine.polisher("llm", &NO_PARAMS).await.unwrap();
    let polished = polisher.polish(&PolishRequest { system: "s", examples: &[], text: "hi" }).await;
    assert_eq!(polished.err().unwrap().kind, ErrorKind::BadResponse);

    let clip = AudioClip::new(vec![0.1; 1600]);
    let transcriber = engine.transcriber("asr").await.unwrap();
    let heard = transcriber.transcribe(&AsrRequest { clip: &clip, language: None, hotwords: &[] }).await;
    assert_eq!(heard.err().unwrap().kind, ErrorKind::BadResponse);
}
