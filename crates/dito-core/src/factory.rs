//! Turns the current [`Config`] into concrete engines.

use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;

use crate::config::{
    CloudAsrConfigurations, CloudPolishConfiguration, CloudPolishTarget, Config, EngineConfig, LocalPolishParams,
    Source,
};
use crate::error::{ErrorKind, ProviderError, ProviderResult};
use crate::health::EngineStatus;
use crate::provider::bedrock::{BedrockEngine, BedrockParams};
use crate::provider::configured_asr::ConfiguredAsr;
use crate::provider::mock::{MockPolisher, MockRouter, MockTranscriber};
use crate::provider::openai::{Flavor, OpenAiEngine};
use crate::provider::{Polisher, Router, Transcriber};
use crate::secrets::SecretStore;

#[async_trait]
pub trait LocalBackend: Send + Sync {
    /// Local speech models decode greedily and take no parameters.
    async fn transcriber(&self, model_id: &str) -> ProviderResult<Arc<dyn Transcriber>>;
    /// A field the user leaves unset takes the value the model's catalog entry recommends.
    async fn polisher(&self, model_id: &str, params: &LocalPolishParams) -> ProviderResult<Arc<dyn Polisher>>;
    /// Decides intents with the chat model `model_id`, the same one that polishes.
    async fn router(&self, model_id: &str) -> ProviderResult<Arc<dyn Router>>;
    fn status(&self, model_id: &str) -> EngineStatus;
}

/// What the pipeline asks for engines. Tests and `--mock` runs substitute [`FixedEngines`].
#[async_trait]
pub trait Engines: Send + Sync {
    async fn transcriber(&self, cfg: &Config) -> ProviderResult<Arc<dyn Transcriber>>;
    /// `None` when polishing is switched off.
    async fn polisher(&self, cfg: &Config) -> ProviderResult<Option<Arc<dyn Polisher>>>;
    /// Command mode routes with a local chat model whatever the polish source is: the instruction never leaves the Mac.
    async fn router(&self, cfg: &Config) -> ProviderResult<Arc<dyn Router>>;
}

type Slot<T> = Option<(EngineConfig, Arc<T>)>;

/// Cloud engines reused across dictations so HTTP connections and cloud credentials survive between them.
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
    speech_config: Option<serde_json::Value>,
    router: Option<Arc<dyn Router>>,
    cache: Mutex<EngineCache>,
}

impl EngineFactory {
    pub fn new(secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            secrets,
            local: None,
            bedrock_endpoint: None,
            speech_config: None,
            router: None,
            cache: Mutex::default(),
        }
    }

    /// Test hook: routing uses this router instead of the local chat model. `--mock` runs have no model to route with.
    pub fn with_router(mut self, router: Arc<dyn Router>) -> Self {
        self.router = Some(router);
        self
    }

    /// Test hook: cloud speech uses this request JSON, without a key, instead of the saved configuration. `--mock`
    /// points speech at its in-process server this way, so the settings keep showing the bundled templates.
    pub fn with_speech_config(mut self, config: Option<serde_json::Value>) -> Self {
        self.speech_config = config;
        self
    }

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
    async fn cloud_polisher(&self, configuration: &CloudPolishConfiguration) -> ProviderResult<Arc<dyn Polisher>> {
        let key = || self.secrets.get(&configuration.account()).unwrap_or_default();
        let params = configuration.params.clone();
        Ok(match &configuration.target {
            CloudPolishTarget::Azure { endpoint, deployment } => Arc::new(OpenAiEngine::new(
                Flavor::Azure { endpoint: endpoint.clone(), deployment: deployment.clone() },
                key(),
                params,
            )?),
            CloudPolishTarget::Compat { base_url, model } => Arc::new(OpenAiEngine::new(
                Flavor::Compat { base_url: base_url.clone(), model: model.clone() },
                key(),
                params,
            )?),
            CloudPolishTarget::Bedrock { profile, region, model_id } => Arc::new(
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

    /// Takes an explicit engine config so the connection test can build from it.
    pub async fn transcriber_for(&self, e: &EngineConfig) -> ProviderResult<Arc<dyn Transcriber>> {
        match e.source {
            Source::Local => self.require_local()?.transcriber(&e.local_model).await,
            Source::Provider => {
                if let Some(config) = &self.speech_config {
                    return Ok(Arc::new(ConfiguredAsr::new(config.clone(), String::new())?));
                }
                // `cloud_asr` stays `None` until a configuration list is first saved; the bundled configurations
                // stand in.
                let bundled;
                let configurations = match &e.cloud_asr {
                    Some(configurations) => configurations,
                    None => {
                        bundled = CloudAsrConfigurations::default();
                        &bundled
                    }
                };
                let active = configurations.active().ok_or_else(|| {
                    ProviderError::new(ErrorKind::Unavailable, "no cloud speech configuration is selected")
                })?;
                let key = self.secrets.get(&active.account()).unwrap_or_default();
                Ok(Arc::new(ConfiguredAsr::new(active.config.clone(), key)?))
            }
        }
    }

    pub async fn polisher_for(&self, e: &EngineConfig) -> ProviderResult<Arc<dyn Polisher>> {
        match e.source {
            // Local engines are not cached here, so a changed parameter applies to the next dictation.
            Source::Local => self.require_local()?.polisher(&e.local_model, &e.local_params_for(&e.local_model)).await,
            Source::Provider => {
                // `cloud_polish` stays `None` until a list is first saved; the templates stand in.
                let active = e.active_polish().ok_or_else(|| {
                    ProviderError::new(ErrorKind::Unavailable, "no cloud polish configuration is selected")
                })?;
                self.cloud_polisher(&active).await
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

    async fn router(&self, cfg: &Config) -> ProviderResult<Arc<dyn Router>> {
        if let Some(router) = &self.router {
            return Ok(router.clone());
        }
        self.require_local()?.router(&cfg.polish.engine.local_model).await
    }
}

pub struct FixedEngines {
    pub asr: Arc<dyn Transcriber>,
    pub polish: Arc<dyn Polisher>,
    pub router: Arc<dyn Router>,
}

impl FixedEngines {
    /// Routes every instruction to polish, confidently.
    pub fn new(asr: Arc<dyn Transcriber>, polish: Arc<dyn Polisher>) -> Self {
        Self { asr, polish, router: Arc::new(MockRouter::default()) }
    }

    pub fn demo() -> Self {
        Self::new(
            Arc::new(
                MockTranscriber::new(
                    "えーと 明日の3時に会議、いや4時に会議があるので、資料を、あの、事前に共有しておいてください",
                )
                .with_delay(std::time::Duration::from_millis(900)),
            ),
            Arc::new(MockPolisher::default().with_delay(std::time::Duration::from_millis(700))),
        )
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

    async fn router(&self, _: &Config) -> ProviderResult<Arc<dyn Router>> {
        Ok(self.router.clone())
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
        let lang = cfg.general.lang();
        let engine = &mut cfg.polish.engine;
        engine.source = Source::Provider;
        let configurations = engine.polish_configurations_mut(lang);
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
    async fn an_unchanged_cloud_polish_configuration_reuses_the_polish_engine() {
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

        let lang = base.general.lang();
        let mut warmer = base.clone();
        warmer.polish.engine.polish_configurations_mut(lang).active_mut().unwrap().params.temperature = Some(0.9);
        let second = polisher(&factory, &warmer).await;
        assert!(!Arc::ptr_eq(&first, &second), "a parameter is part of the engine");

        let mut other = warmer.clone();
        let copy = other.polish.engine.polish_configurations_mut(lang).duplicate("compat", "Copy").unwrap();
        assert_eq!(other.polish.engine.polish_configurations(lang).selected, copy);
        let third = polisher(&factory, &other).await;
        assert!(!Arc::ptr_eq(&second, &third), "another configuration is another engine");
    }

    #[derive(Default)]
    struct Recorder(Mutex<Vec<(String, LocalPolishParams)>>);

    #[async_trait]
    impl LocalBackend for Recorder {
        async fn transcriber(&self, _: &str) -> ProviderResult<Arc<dyn Transcriber>> {
            Err(ProviderError::new(ErrorKind::Unavailable, "speech is not under test"))
        }
        async fn polisher(&self, model_id: &str, params: &LocalPolishParams) -> ProviderResult<Arc<dyn Polisher>> {
            self.0.lock().unwrap().push((model_id.into(), *params));
            Ok(Arc::new(MockPolisher::default()))
        }
        async fn router(&self, _: &str) -> ProviderResult<Arc<dyn Router>> {
            Err(ProviderError::new(ErrorKind::Unavailable, "routing is not under test"))
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
        let params = LocalPolishParams { temperature: Some(0.5), top_k: Some(20), ..LocalPolishParams::default() };
        cfg.polish.engine.set_local_params(&model, params);
        cfg.polish
            .engine
            .set_local_params("another-model", LocalPolishParams { max_tokens: Some(9), ..Default::default() });

        factory.polisher(&cfg).await.unwrap().expect("polish is enabled");
        cfg.polish.engine.set_local_params(&model, LocalPolishParams::default());
        factory.polisher(&cfg).await.unwrap().expect("polish is enabled");

        let asked = backend.0.lock().unwrap().clone();
        assert_eq!(
            asked,
            [(model.clone(), params), (model, LocalPolishParams::default())],
            "nothing is cached for local"
        );
    }

    #[tokio::test]
    async fn a_selection_that_points_nowhere_is_a_reported_problem_not_a_default() {
        let mut cfg = compat_config("model-a");
        cfg.polish.engine.polish_configurations_mut(cfg.general.lang()).selected = "gone".into();
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
