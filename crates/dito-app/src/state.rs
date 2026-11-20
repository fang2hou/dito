//! Shared services, wired once at startup and handed to every window.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use dito_audio::{AutoOverrides, Capture, LiveCapture};
use dito_core::audio::AudioClip;
use dito_core::config::{ConfigStore, default_data_dir};
use dito_core::controller::{self, Deps, DictationHandle, Env, Recorder, RecorderError, SelectionSource, Sink};
use dito_core::factory::{EngineFactory, Engines};
use dito_core::local_api::{self, LocalApi};
use dito_core::secrets::{MemorySecrets, SecretStore};
use dito_core::store::Store;
use dito_core::transcription::{self, TranscriptionDeps, TranscriptionHandle, VoiceDetector, VoiceDetectors};
#[cfg(not(feature = "runtimes"))]
use dito_local::NoRuntime;
use dito_local::{LocalEngines, ModelManager, RuntimeLoader};
use dito_platform::{KeychainSecrets, MacFrontmost, MacMusicMute, MacSink, Permission, Player, Status, permissions};
use gpui_kit::Global;

use crate::dev::log_sink::LogSink;
#[cfg(feature = "mock")]
use crate::dev::mock_router::KeywordRouter;
#[cfg(feature = "mock")]
use crate::dev::mock_selection::MockSelection;
#[cfg(feature = "mock")]
use crate::dev::seed;
#[cfg(feature = "mock")]
use crate::dev::synthetic_recorder::SyntheticRecorder;
use crate::dev::wav_recorder::WavRecorder;
use crate::hotkeys::Hotkeys;
use crate::logbook::LogBook;
use crate::mac_focus_warmer::MacFocusWarmer;
use crate::mac_selection::MacSelection;
use crate::opts::Opts;
use crate::services::InstalledCatalog;

pub struct AppState {
    pub opts: Opts,
    pub cfg: Arc<ConfigStore>,
    pub store: Arc<Store>,
    pub models: ModelManager,
    pub local: Arc<LocalEngines>,
    pub factory: Arc<EngineFactory>,
    pub secrets: Arc<dyn SecretStore>,
    pub dictation: DictationHandle,
    pub transcription: TranscriptionHandle,
    pub local_api: LocalApi,
    /// Dictation aimed at one of Dito's own text fields, answered on the UI thread.
    pub own_inserts: crate::services::OwnWindowInserts,
    pub recorder: Arc<dyn Recorder>,
    pub capture: Option<Arc<Capture>>,
    pub player: Player,
    pub hotkeys: Arc<Hotkeys>,
    pub rt: tokio::runtime::Handle,
    /// The app's log lines since launch and the switch for the detailed log.
    pub log: Arc<LogBook>,
    pub chat_model: crate::services::ChatModelKeeper,
    #[cfg(feature = "mock")]
    pub _mock_server: Option<dito_testkit::MockServer>,
}

impl Global for AppGlobal {}
pub struct AppGlobal(pub Arc<AppState>);

impl AppState {
    /// Takes effect with the next recording.
    pub fn recorder_device(&self, name: &str) {
        if let Some(c) = &self.capture {
            c.set_device(name);
        }
    }

    /// Takes effect with the next recording.
    pub fn recorder_auto(&self, overrides: &AutoOverrides) {
        if let Some(c) = &self.capture {
            c.set_auto_overrides(overrides);
        }
    }

    /// A data file this build cannot use fails the build with its `ConfigError` or `StoreError` intact in the error,
    /// so the caller decides which of them get a window of their own (see `startup_failure`).
    pub fn build(opts: Opts, rt: &tokio::runtime::Handle) -> Result<Arc<Self>> {
        let _guard = rt.enter();
        let data_dir = data_dir(&opts);
        std::fs::create_dir_all(&data_dir)?;
        let cfg = Arc::new(ConfigStore::open(data_dir.join(CONFIG_FILE))?);

        #[cfg(feature = "mock")]
        let mock_server = start_mock_server(&opts, &cfg, rt)?;

        let store = open_store(&opts, &data_dir)?;
        let models = ModelManager::new(cfg.clone());
        let local = LocalEngines::new(models.clone(), runtime_loader());
        let secrets: Arc<dyn SecretStore> =
            if opts.uses_keychain() { Arc::new(KeychainSecrets::new()) } else { Arc::new(MemorySecrets::default()) };
        #[cfg(feature = "mock")]
        let mock_speech = mock_server.as_ref().map(|server| dito_testkit::mock_cloud_asr_config(&server.url));
        #[cfg(not(feature = "mock"))]
        let mock_speech = None;
        let factory = EngineFactory::new(secrets.clone())
            .with_local(local.clone())
            .with_bedrock_endpoint(std::env::var("DITO_BEDROCK_ENDPOINT").ok())
            .with_speech_config(mock_speech);
        #[cfg(feature = "mock")]
        let factory = if opts.mock { factory.with_router(Arc::new(KeywordRouter)) } else { factory };
        let factory = Arc::new(factory);
        let capture = open_capture(&opts, &cfg);
        let recorder = pick_recorder(&opts, capture.as_ref())?;
        let sink: Arc<dyn Sink> = if opts.log_sink { Arc::new(LogSink) } else { Arc::new(MacSink) };
        let (sink, own_inserts) = crate::services::own_window_sink(sink);
        let engines: Arc<dyn Engines> = factory.clone();
        let microphone_cfg = cfg.clone();
        let transcription = transcription::spawn(TranscriptionDeps {
            audio: Arc::new(LiveCapture::new(move || {
                let cfg = microphone_cfg.get();
                (cfg.general.microphone.clone(), cfg.general.microphone_auto.clone())
            })),
            detectors: Arc::new(ModelDetectors { models: models.clone() }),
            engines: engines.clone(),
            config: cfg.clone(),
            store: store.clone(),
        });
        let local_api = LocalApi::spawn(
            local_api::Deps {
                cfg: cfg.clone(),
                token_path: data_dir.join("local_api.token"),
                backend: factory.clone(),
                installed: Arc::new(InstalledCatalog(models.clone())),
            },
            rt,
        );
        let dictation = controller::spawn(Deps {
            env: Env {
                recorder: recorder.clone(),
                sink,
                frontmost: Arc::new(MacFrontmost),
                focus_warmer: Arc::new(MacFocusWarmer),
                music_mute: Arc::new(MacMusicMute),
                selection: pick_selection(&opts),
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
            transcription,
            local_api,
            own_inserts,
            recorder,
            capture,
            player: Player::default(),
            hotkeys: Arc::new(Hotkeys::new()),
            rt: rt.clone(),
            log: LogBook::shared(),
            chat_model: Default::default(),
            #[cfg(feature = "mock")]
            _mock_server: mock_server,
        }))
    }
}

const CONFIG_FILE: &str = "config.toml";
const DB_FILE: &str = "dito.db";

pub(crate) fn data_dir(opts: &Opts) -> PathBuf {
    match &opts.data_dir {
        Some(dir) => dir.clone(),
        None if opts.mock => std::env::temp_dir().join("dito-mock"),
        None => default_data_dir(),
    }
}

/// The history database of this run, for errors that do not name their file.
pub(crate) fn database_path(opts: &Opts) -> PathBuf {
    data_dir(opts).join(DB_FILE)
}

#[cfg(feature = "mock")]
fn start_mock_server(
    opts: &Opts,
    cfg: &ConfigStore,
    rt: &tokio::runtime::Handle,
) -> Result<Option<dito_testkit::MockServer>> {
    if !opts.mock {
        return Ok(None);
    }
    let server = rt.block_on(dito_testkit::MockServer::start())?;
    cfg.update(|c| {
        let fresh = dito_core::config::Config::dev_azure();
        c.asr = fresh.asr;
        c.text_model = fresh.text_model;
        if let Some(azure) = c.text_model.text_configurations_mut(c.general.lang()).active_mut() {
            *azure.target.address_mut() = server.url.clone();
        }
        c.onboarded = true;
    })?;
    Ok(Some(server))
}

fn open_store(opts: &Opts, data_dir: &std::path::Path) -> Result<Arc<Store>> {
    let store = Arc::new(Store::open(data_dir.join(DB_FILE), dito_platform::local_utc_offset_secs())?);
    #[cfg(feature = "mock")]
    if opts.mock {
        seed::demo(&store, crate::timefmt::now())?;
    }
    #[cfg(not(feature = "mock"))]
    let _ = opts;
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

/// Transcription detectors from the downloaded Silero model. Without the on-device runtimes (`--mock` builds) a
/// level-based detector stands in.
struct ModelDetectors {
    #[cfg_attr(not(feature = "runtimes"), allow(dead_code))]
    models: ModelManager,
}

impl VoiceDetectors for ModelDetectors {
    #[cfg(feature = "runtimes")]
    fn create(&self) -> Result<Box<dyn VoiceDetector>, String> {
        let dir = self.models.model_dir(dito_local::SILERO_VAD);
        dito_runtimes::SileroDetectors::new(&dir.join("silero_vad.onnx")).create()
    }

    #[cfg(not(feature = "runtimes"))]
    fn create(&self) -> Result<Box<dyn VoiceDetector>, String> {
        transcription::EnergyDetectors.create()
    }
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
        (None, Some(capture)) => Arc::new(AuthorizedCapture(capture.clone())),
        #[cfg(feature = "mock")]
        (None, None) => Arc::new(SyntheticRecorder::default()),
        // Only `--mock` without `--real-mic` leaves no capture, and `--mock` needs the `mock` feature.
        #[cfg(not(feature = "mock"))]
        (None, None) => anyhow::bail!("no audio input is available"),
    })
}

/// `--mock` never reads or writes another app's text.
fn pick_selection(opts: &Opts) -> Arc<dyn SelectionSource> {
    #[cfg(feature = "mock")]
    if opts.mock {
        return Arc::new(MockSelection::from_env());
    }
    let _ = opts;
    Arc::new(MacSelection)
}

struct AuthorizedCapture(Arc<Capture>);

impl Recorder for AuthorizedCapture {
    fn start(&self) -> Result<(), RecorderError> {
        // Opening an input stream can prompt before the user reaches the permission controls.
        if permissions::status(Permission::Microphone) != Status::Granted {
            return Err(RecorderError::Device("microphone permission has not been granted".into()));
        }
        self.0.start()
    }

    fn stop(&self) -> AudioClip {
        self.0.stop()
    }

    fn level(&self) -> f32 {
        self.0.level()
    }
}
