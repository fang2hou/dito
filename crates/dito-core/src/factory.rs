//! Turns the current [`Config`] into concrete engines.

use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;

use crate::config::{AsrProfiles, Config, EngineConfig, LocalParams, PolishConfiguration, PolishTarget, Source};
use crate::error::{ErrorKind, ProviderError, ProviderResult};
use crate::health::EngineStatus;
use crate::provider::bedrock::{BedrockEngine, BedrockParams};
use crate::provider::configured_asr::ConfiguredAsr;
use crate::provider::mock::{MockPolisher, MockTranscriber};
use crate::provider::openai::{Flavor, OpenAiEngine};
use crate::provider::{Polisher, Transcriber};
use crate::secrets::SecretStore;

/// On-device engines, implemented by `dito-local`.
#[async_trait]
pub trait LocalBackend: Send + Sync {
    /// Local speech models decode greedily and take no parameters.
    async fn transcriber(&self, model_id: &str) -> ProviderResult<Arc<dyn Transcriber>>;
    /// `params` are the user's sampling settings for this model; a field they leave unset takes the value the model's
    /// catalog entry recommends.
    async fn polisher(&self, model_id: &str, params: &LocalParams) -> ProviderResult<Arc<dyn Polisher>>;
    fn status(&self, model_id: &str) -> EngineStatus;
}

/// What the pipeline asks for engines. Tests and `--mock` runs substitute [`FixedEngines`].
#[async_trait]
pub trait Engines: Send + Sync {
    async fn transcriber(&self, cfg: &Config) -> ProviderResult<Arc<dyn Transcriber>>;
    /// `None` when polishing is switched off.
    async fn polisher(&self, cfg: &Config) -> ProviderResult<Option<Arc<dyn Polisher>>>;
}

/// An engine together with the config it was built from.
type Slot<T> = Option<(EngineConfig, Arc<T>)>;

/// Provider engines reused across dictations so HTTP connections and cloud credentials survive between them.
/// The API key is not part of the key; `invalidate_engines` must run whenever a secret changes.
#[derive(Default)]
struct EngineCache {
    /// Bumped by every invalidation so a build that raced with one is not stored.
    epoch: u64,
    asr: Slot<dyn Transcriber>,
    polish: Slot<dyn Polisher>,
}

pub struct EngineFactory {
    secrets: Arc<dyn SecretStore>,
    local: Option<Arc<dyn LocalBackend>>,
    bedrock_endpoint: Option<String>,
    speech_profile: Option<serde_json::Value>,
    cache: Mutex<EngineCache>,
}

impl EngineFactory {
    pub fn new(secrets: Arc<dyn SecretStore>) -> Self {
        Self { secrets, local: None, bedrock_endpoint: None, speech_profile: None, cache: Mutex::default() }
    }

    /// Test hook: provider speech uses this connection config, without a key, instead of the saved profile. `--mock`
    /// points speech at its in-process server this way, so the settings keep showing the bundled templates.
    pub fn with_speech_profile(mut self, config: Option<serde_json::Value>) -> Self {
        self.speech_profile = config;
        self
    }

    /// Drop the cached engines; the next dictation builds them again from the current config and secrets.
    pub fn invalidate_engines(&self) {
        let mut cache = self.lock_cache();
        cache.epoch += 1;
        cache.asr = None;
        cache.polish = None;
    }

    fn lock_cache(&self) -> MutexGuard<'_, EngineCache> {
        self.cache.lock().expect("engine cache mutex poisoned")
    }

    pub fn with_local(mut self, local: Arc<dyn LocalBackend>) -> Self {
        self.local = Some(local);
        self
    }

    pub fn with_bedrock_endpoint(mut self, url: Option<String>) -> Self {
        self.bedrock_endpoint = url;
        self
    }

    /// The engine of one cloud polish configuration. Azure and compatible servers use the key stored under the
    /// configuration's Keychain account; Bedrock authenticates through its AWS profile.
    async fn cloud_polisher(&self, configuration: &PolishConfiguration) -> ProviderResult<Arc<dyn Polisher>> {
        let key = || self.secrets.get(&configuration.account()).unwrap_or_default();
        let params = configuration.params.clone();
        Ok(match &configuration.target {
            PolishTarget::Azure { endpoint, deployment } => Arc::new(OpenAiEngine::new(
                Flavor::Azure { endpoint: endpoint.clone(), deployment: deployment.clone() },
                key(),
                params,
            )?),
            PolishTarget::Compat { base_url, model } => Arc::new(OpenAiEngine::new(
                Flavor::Compat { base_url: base_url.clone(), model: model.clone() },
                key(),
                params,
            )?),
            PolishTarget::Bedrock { profile, region, model_id } => Arc::new(
                BedrockEngine::connect(BedrockParams {
                    profile: profile.clone(),
                    region: region.clone(),
                    model_id: model_id.clone(),
                    params,
                    endpoint_override: self.bedrock_endpoint.clone(),
                })
                .await?,
            ),
        })
    }

    fn require_local(&self) -> ProviderResult<&Arc<dyn LocalBackend>> {
        self.local
            .as_ref()
            .ok_or_else(|| ProviderError::new(ErrorKind::Unavailable, "local engines are not available in this build"))
    }

    /// Build the ASR engine for an explicit engine config (used by the connection test too).
    pub async fn transcriber_for(&self, e: &EngineConfig) -> ProviderResult<Arc<dyn Transcriber>> {
        match e.source {
            Source::Local => self.require_local()?.transcriber(&e.local_model).await,
            Source::Provider => {
                if let Some(config) = &self.speech_profile {
                    return Ok(Arc::new(ConfiguredAsr::new(config.clone(), String::new())?));
                }
                // `cloud_asr` stays `None` until a profile list is first saved; the bundled profiles stand in.
                let bundled;
                let profiles = match &e.cloud_asr {
                    Some(profiles) => profiles,
                    None => {
                        bundled = AsrProfiles::default();
                        &bundled
                    }
                };
                let key = self.secrets.get(&profiles.account()).unwrap_or_default();
                Ok(Arc::new(ConfiguredAsr::new(profiles.active().clone(), key)?))
            }
        }
    }

    pub async fn polisher_for(&self, e: &EngineConfig) -> ProviderResult<Arc<dyn Polisher>> {
        match e.source {
            // Local engines are not cached here, so a changed parameter applies to the next dictation.
            Source::Local => self.require_local()?.polisher(&e.local_model, &e.local_params_for(&e.local_model)).await,
            Source::Provider => {
                // `cloud_polish` stays `None` until a list is first saved; the templates stand in.
                let configurations = e.polish_configurations();
                let active = configurations.active().ok_or_else(|| {
                    ProviderError::new(ErrorKind::Unavailable, "no cloud polish configuration is selected")
                })?;
                self.cloud_polisher(active).await
            }
        }
    }
}

#[async_trait]
impl Engines for EngineFactory {
    async fn transcriber(&self, cfg: &Config) -> ProviderResult<Arc<dyn Transcriber>> {
        let e = &cfg.asr;
        let cacheable = e.source == Source::Provider;
        let epoch = {
            let cache = self.lock_cache();
            if cacheable
                && let Some((built_from, engine)) = &cache.asr
                && built_from == e
            {
                return Ok(engine.clone());
            }
            cache.epoch
        };
        let engine = self.transcriber_for(e).await?;
        if cacheable {
            let mut cache = self.lock_cache();
            if cache.epoch == epoch {
                cache.asr = Some((e.clone(), engine.clone()));
            }
        }
        Ok(engine)
    }

    async fn polisher(&self, cfg: &Config) -> ProviderResult<Option<Arc<dyn Polisher>>> {
        if !cfg.polish.enabled {
            return Ok(None);
        }
        let e = &cfg.polish.engine;
        let cacheable = e.source == Source::Provider;
        let epoch = {
            let cache = self.lock_cache();
            if cacheable
                && let Some((built_from, engine)) = &cache.polish
                && built_from == e
            {
                return Ok(Some(engine.clone()));
            }
            cache.epoch
        };
        let engine = self.polisher_for(e).await?;
        if cacheable {
            let mut cache = self.lock_cache();
            if cache.epoch == epoch {
                cache.polish = Some((e.clone(), engine.clone()));
            }
        }
        Ok(Some(engine))
    }
}

/// Fixed engines regardless of config.
pub struct FixedEngines {
    pub asr: Arc<dyn Transcriber>,
    pub polish: Arc<dyn Polisher>,
}

impl FixedEngines {
    /// Offline demo engines used by `--mock`.
    pub fn demo() -> Self {
        Self {
            asr: Arc::new(
                MockTranscriber::new(
                    "えーと 明日の3時に会議、いや4時に会議があるので、資料を、あの、事前に共有しておいてください",
                )
                .with_delay(std::time::Duration::from_millis(900)),
            ),
            polish: Arc::new(MockPolisher::default().with_delay(std::time::Duration::from_millis(700))),
        }
    }
}

#[async_trait]
impl Engines for FixedEngines {
    async fn transcriber(&self, _: &Config) -> ProviderResult<Arc<dyn Transcriber>> {
        Ok(self.asr.clone())
    }

    async fn polisher(&self, cfg: &Config) -> ProviderResult<Option<Arc<dyn Polisher>>> {
        Ok(cfg.polish.enabled.then(|| self.polish.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemorySecrets;

    fn factory() -> EngineFactory {
        EngineFactory::new(Arc::new(MemorySecrets::default()))
    }

    fn compat_config(model: &str) -> Config {
        let mut cfg = Config::default();
        cfg.polish.enabled = true;
        let engine = &mut cfg.polish.engine;
        engine.source = Source::Provider;
        let configurations = engine.polish_configurations_mut();
        configurations.select("compat");
        let compat = configurations.active_mut().expect("the templates include a compat configuration");
        *compat.target.address_mut() = "http://127.0.0.1:1/v1".into();
        *compat.target.model_mut() = model.into();
        cfg
    }

    async fn polisher(factory: &EngineFactory, cfg: &Config) -> Arc<dyn Polisher> {
        factory.polisher(cfg).await.expect("compat engine builds offline").expect("polish is enabled")
    }

    #[tokio::test]
    async fn unchanged_provider_config_reuses_the_polish_engine() {
        let factory = factory();
        let cfg = compat_config("model-a");
        let first = polisher(&factory, &cfg).await;
        let second = polisher(&factory, &cfg).await;
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[tokio::test]
    async fn a_changed_engine_config_builds_a_new_engine() {
        let factory = factory();
        let first = polisher(&factory, &compat_config("model-a")).await;
        let second = polisher(&factory, &compat_config("model-b")).await;
        assert!(!Arc::ptr_eq(&first, &second));
        let third = polisher(&factory, &compat_config("model-b")).await;
        assert!(Arc::ptr_eq(&second, &third));
    }

    #[tokio::test]
    async fn a_changed_parameter_or_selection_builds_a_new_engine() {
        let factory = factory();
        let base = compat_config("model-a");
        let first = polisher(&factory, &base).await;

        let mut warmer = base.clone();
        warmer.polish.engine.polish_configurations_mut().active_mut().unwrap().params.temperature = Some(0.9);
        let second = polisher(&factory, &warmer).await;
        assert!(!Arc::ptr_eq(&first, &second), "a parameter is part of the engine");

        let mut other = warmer.clone();
        let copy = other.polish.engine.polish_configurations_mut().duplicate("compat", "Copy").unwrap();
        assert_eq!(other.polish.engine.polish_configurations().selected, copy);
        let third = polisher(&factory, &other).await;
        assert!(!Arc::ptr_eq(&second, &third), "another configuration is another engine");
    }

    /// A local backend that remembers which model it was asked to polish with, and under which parameters.
    #[derive(Default)]
    struct Recorder(Mutex<Vec<(String, LocalParams)>>);

    #[async_trait]
    impl LocalBackend for Recorder {
        async fn transcriber(&self, _: &str) -> ProviderResult<Arc<dyn Transcriber>> {
            Err(ProviderError::new(ErrorKind::Unavailable, "speech is not under test"))
        }
        async fn polisher(&self, model_id: &str, params: &LocalParams) -> ProviderResult<Arc<dyn Polisher>> {
            self.0.lock().unwrap().push((model_id.into(), *params));
            Ok(Arc::new(MockPolisher::default()))
        }
        fn status(&self, _: &str) -> EngineStatus {
            EngineStatus::Running
        }
    }

    #[tokio::test]
    async fn a_local_polisher_gets_the_parameters_saved_for_its_model_and_for_no_other() {
        let backend = Arc::new(Recorder::default());
        let factory = factory().with_local(backend.clone());
        let mut cfg = Config::default();
        let model = cfg.polish.engine.local_model.clone();
        let params = LocalParams { temperature: Some(0.5), top_k: Some(20), ..LocalParams::default() };
        cfg.polish.engine.set_local_params(&model, params);
        cfg.polish.engine.set_local_params("another-model", LocalParams { max_tokens: Some(9), ..Default::default() });

        factory.polisher(&cfg).await.unwrap().expect("polish is enabled");
        cfg.polish.engine.set_local_params(&model, LocalParams::default());
        factory.polisher(&cfg).await.unwrap().expect("polish is enabled");

        let asked = backend.0.lock().unwrap().clone();
        assert_eq!(asked, [(model.clone(), params), (model, LocalParams::default())], "nothing is cached for local");
    }

    #[tokio::test]
    async fn a_selection_that_points_nowhere_is_a_reported_problem_not_a_default() {
        let mut cfg = compat_config("model-a");
        cfg.polish.engine.polish_configurations_mut().selected = "gone".into();
        let error = factory().polisher(&cfg).await.err().expect("nothing is selected");
        assert_eq!(error.kind, ErrorKind::Unavailable);
    }

    #[tokio::test]
    async fn invalidating_forces_a_rebuild() {
        let factory = factory();
        let cfg = compat_config("model-a");
        let first = polisher(&factory, &cfg).await;
        factory.invalidate_engines();
        let second = polisher(&factory, &cfg).await;
        assert!(!Arc::ptr_eq(&first, &second));
    }
}
