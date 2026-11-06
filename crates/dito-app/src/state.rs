//! Shared services, wired once at startup and handed to every window.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use dito_audio::{AutoOverrides, Capture};
use dito_core::config::{ConfigStore, default_data_dir};
use dito_core::controller::{self, Deps, DictationHandle, Env, Recorder, Sink};
use dito_core::factory::{EngineFactory, Engines};
use dito_core::secrets::{MemorySecrets, SecretStore};
use dito_core::store::Store;
#[cfg(not(feature = "runtimes"))]
use dito_local::NoRuntime;
use dito_local::{LocalEngines, ModelManager, RuntimeLoader};
use dito_platform::{KeychainSecrets, MacFrontmost, MacMusicMute, MacSink, Player};
use gpui_kit::Global;

use crate::dev::log_sink::LogSink;
use crate::dev::seed;
use crate::dev::synthetic_recorder::SyntheticRecorder;
use crate::dev::wav_recorder::WavRecorder;
use crate::hotkeys::Hotkeys;
use crate::mac_focus_warmer::MacFocusWarmer;
use crate::opts::Opts;

pub struct AppState {
    pub opts: Opts,
    pub cfg: Arc<ConfigStore>,
    pub store: Arc<Store>,
    pub models: ModelManager,
    pub local: Arc<LocalEngines>,
    pub factory: Arc<EngineFactory>,
    pub secrets: Arc<dyn SecretStore>,
    pub dictation: DictationHandle,
    pub recorder: Arc<dyn Recorder>,
    pub capture: Option<Arc<Capture>>,
    pub player: Player,
    pub hotkeys: Arc<Hotkeys>,
    pub rt: tokio::runtime::Handle,
    #[cfg(feature = "mock")]
    pub _mock_server: Option<dito_testkit::MockServer>,
}

impl Global for AppGlobal {}
pub struct AppGlobal(pub Arc<AppState>);

impl AppState {
    /// Switch the capture device used by the next recording.
    pub fn recorder_device(&self, name: &str) {
        if let Some(c) = &self.capture {
            c.set_device(name);
        }
    }

    /// Set which devices automatic microphone picking may open, from the next recording on.
    pub fn recorder_auto(&self, overrides: &AutoOverrides) {
        if let Some(c) = &self.capture {
            c.set_auto_overrides(overrides);
        }
    }

    pub fn build(opts: Opts, rt: &tokio::runtime::Runtime) -> Result<Arc<Self>> {
        let _guard = rt.enter();
        let data_dir = data_dir(&opts);
        std::fs::create_dir_all(&data_dir)?;
        let cfg = Arc::new(ConfigStore::open(data_dir.join("config.toml"))?);

        #[cfg(feature = "mock")]
        let mock_server = start_mock_server(&opts, &cfg, rt)?;

        let store = open_store(&opts, &data_dir)?;
        let models = ModelManager::new(cfg.clone());
        let local = LocalEngines::new(models.clone(), runtime_loader());
        let secrets: Arc<dyn SecretStore> =
            if opts.mock { Arc::new(MemorySecrets::default()) } else { Arc::new(KeychainSecrets::new()) };
        let factory = Arc::new(
            EngineFactory::new(secrets.clone())
                .with_local(local.clone())
                .with_bedrock_endpoint(std::env::var("DITO_BEDROCK_ENDPOINT").ok()),
        );
        let capture = open_capture(&opts, &cfg);
        let recorder = pick_recorder(&opts, capture.as_ref())?;
        let sink: Arc<dyn Sink> = if opts.log_sink { Arc::new(LogSink) } else { Arc::new(MacSink) };
        let engines: Arc<dyn Engines> = factory.clone();
        let dictation = controller::spawn(Deps {
            env: Env {
                recorder: recorder.clone(),
                sink,
                frontmost: Arc::new(MacFrontmost),
                focus_warmer: Arc::new(MacFocusWarmer),
                music_mute: Arc::new(MacMusicMute),
            },
            config: cfg.clone(),
            store: store.clone(),
            engines,
            audio_dir: Some(data_dir.join("audio")),
        });
        Ok(Arc::new(Self {
            opts,
            cfg,
            store,
            models,
            local,
            factory,
            secrets,
            dictation,
            recorder,
            capture,
            player: Player::default(),
            hotkeys: Arc::new(Hotkeys::new()),
            rt: rt.handle().clone(),
            #[cfg(feature = "mock")]
            _mock_server: mock_server,
        }))
    }
}

fn data_dir(opts: &Opts) -> PathBuf {
    match &opts.data_dir {
        Some(dir) => dir.clone(),
        None if opts.mock => std::env::temp_dir().join("dito-mock"),
        None => default_data_dir(),
    }
}

/// With `--mock`: start the in-process cloud endpoints and point both cloud stages at them.
#[cfg(feature = "mock")]
fn start_mock_server(
    opts: &Opts,
    cfg: &ConfigStore,
    rt: &tokio::runtime::Runtime,
) -> Result<Option<dito_testkit::MockServer>> {
    if !opts.mock {
        return Ok(None);
    }
    let server = rt.block_on(dito_testkit::MockServer::start())?;
    cfg.update(|c| {
        let fresh = dito_core::config::Config::dev_azure();
        c.asr = fresh.asr;
        c.polish.engine = fresh.polish.engine;
        c.asr.provider.azure.endpoint = server.url.clone();
        c.polish.engine.provider.azure.endpoint = server.url.clone();
        c.onboarded = true;
    })?;
    Ok(Some(server))
}

fn open_store(opts: &Opts, data_dir: &std::path::Path) -> Result<Arc<Store>> {
    let store = Arc::new(Store::open(data_dir.join("dito.db"), dito_platform::local_utc_offset_secs())?);
    if opts.mock {
        seed::demo(&store, crate::timefmt::now())?;
    }
    Ok(store)
}

#[cfg(feature = "runtimes")]
fn runtime_loader() -> Arc<dyn RuntimeLoader> {
    Arc::new(dito_runtimes::Runtimes)
}

#[cfg(not(feature = "runtimes"))]
fn runtime_loader() -> Arc<dyn RuntimeLoader> {
    Arc::new(NoRuntime)
}

/// The real microphone, unless `--mock` or `--wav` replaces it (`--real-mic` keeps it under `--mock`).
fn open_capture(opts: &Opts, cfg: &ConfigStore) -> Option<Arc<Capture>> {
    let wanted = (!opts.mock || opts.real_mic) && opts.wav.is_none();
    wanted.then(|| {
        let capture = Arc::new(Capture::new(&cfg.get().general.microphone));
        capture.set_auto_overrides(&cfg.get().general.microphone_auto);
        capture
    })
}

fn pick_recorder(opts: &Opts, capture: Option<&Arc<Capture>>) -> Result<Arc<dyn Recorder>> {
    Ok(match (&opts.wav, capture) {
        (Some(path), _) => Arc::new(WavRecorder::open(path)?),
        (None, Some(capture)) => capture.clone(),
        (None, None) => Arc::new(SyntheticRecorder::default()),
    })
}
