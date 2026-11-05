//! Shared services, wired once at startup and handed to every window.

use std::sync::Arc;

use anyhow::Result;
use dito_audio::Capture;
use dito_core::config::{ConfigStore, default_data_dir};
use dito_core::controller::{self, Deps, DictationHandle, Env, Recorder, Sink};
use dito_core::factory::{EngineFactory, Engines};
use dito_core::secrets::{MemorySecrets, SecretStore};
use dito_core::store::Store;
#[cfg(not(feature = "runtimes"))]
use dito_local::engine::NoRuntime;
use dito_local::{LocalEngines, ModelManager};
use dito_platform::{KeychainSecrets, MacFrontmost, MacMusicMute, MacSink, Player, warm_focus};
use gpui_kit::Global;

// Compatibility re-exports: these lived in `state` before the module split.
pub use crate::fixtures::{SyntheticRecorder, WavRecorder};
pub use crate::opts::Opts;

use crate::fixtures::{LogSink, seed_demo};
use crate::hotkeys::Hotkeys;

/// Warms the insertion target's accessibility tree as soon as recording starts.
struct MacFocusWarmer;
impl dito_core::controller::FocusWarmer for MacFocusWarmer {
    fn warm(&self) {
        warm_focus();
    }
}

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
    pub fn recorder_auto(&self, overrides: &dito_audio::AutoOverrides) {
        if let Some(c) = &self.capture {
            c.set_auto_overrides(overrides);
        }
    }

    pub fn build(opts: Opts, rt: &tokio::runtime::Runtime) -> Result<Arc<Self>> {
        let _guard = rt.enter();
        let data_dir = opts
            .data_dir
            .clone()
            .unwrap_or_else(|| if opts.mock { std::env::temp_dir().join("dito-mock") } else { default_data_dir() });
        std::fs::create_dir_all(&data_dir)?;
        let cfg = Arc::new(ConfigStore::open(data_dir.join("config.toml"))?);

        #[cfg(feature = "mock")]
        let mock_server = if opts.mock { Some(rt.block_on(dito_testkit::MockServer::start())?) } else { None };
        #[cfg(feature = "mock")]
        if let Some(srv) = &mock_server {
            let url = srv.url.clone();
            cfg.update(|c| {
                let fresh = dito_core::config::Config::dev_azure();
                c.asr = fresh.asr;
                c.polish.engine = fresh.polish.engine;
                c.asr.provider.azure.endpoint = url.clone();
                c.polish.engine.provider.azure.endpoint = url;
                c.onboarded = true;
            })?;
        }

        let tz = dito_platform::local_utc_offset_secs();
        let store = Arc::new(Store::open(data_dir.join("dito.db"), tz)?);
        if opts.mock {
            seed_demo(&store, crate::timefmt::now())?;
        }
        let models = ModelManager::new(cfg.clone());
        #[cfg(feature = "runtimes")]
        let loader: Arc<dyn dito_local::RuntimeLoader> = Arc::new(dito_runtimes::Runtimes);
        #[cfg(not(feature = "runtimes"))]
        let loader: Arc<dyn dito_local::RuntimeLoader> = Arc::new(NoRuntime);
        let local = LocalEngines::new(models.clone(), loader);
        let secrets: Arc<dyn SecretStore> =
            if opts.mock { Arc::new(MemorySecrets::default()) } else { Arc::new(KeychainSecrets::new()) };
        let factory = Arc::new(
            EngineFactory::new(secrets.clone())
                .with_local(local.clone())
                .with_bedrock_endpoint(std::env::var("DITO_BEDROCK_ENDPOINT").ok()),
        );
        let engines: Arc<dyn Engines> = factory.clone();
        let capture = ((!opts.mock || opts.real_mic) && opts.wav.is_none()).then(|| {
            let c = Arc::new(Capture::new(&cfg.get().general.microphone));
            c.set_auto_overrides(&cfg.get().general.microphone_auto);
            c
        });
        let recorder: Arc<dyn Recorder> = match (&opts.wav, &capture) {
            (Some(path), _) => Arc::new(WavRecorder::open(path)?),
            (None, Some(c)) => c.clone(),
            (None, None) => Arc::new(SyntheticRecorder::new()),
        };
        let sink: Arc<dyn Sink> = if opts.log_sink { Arc::new(LogSink) } else { Arc::new(MacSink) };
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
