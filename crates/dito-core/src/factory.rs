//! Turns the current [`Config`] into concrete engines.

use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;

use crate::config::{
    CloudAsrConfigurations, CloudTextConfiguration, CloudTextParams, CloudTextTarget, Config, EngineConfig,
    LocalTextParams, Source,
};
use crate::error::{ErrorKind, ProviderError, ProviderResult};
use crate::health::EngineStatus;
use crate::provider::bedrock::{BedrockEngine, BedrockParams};
use crate::provider::configured_asr::ConfiguredAsr;
use crate::provider::mock::{MockRouter, MockTextModel, MockTranscriber};
use crate::provider::openai::{Flavor, OpenAiEngine};
use crate::provider::{Router, TextModel, Transcriber};
use crate::secrets::SecretStore;

#[async_trait]
pub trait LocalBackend: Send + Sync {
    /// Local speech models decode greedily and take no parameters.
    async fn transcriber(&self, model_id: &str) -> ProviderResult<Arc<dyn Transcriber>>;
    /// A field the user leaves unset takes the value the model's catalog entry recommends.
    async fn text_model(&self, model_id: &str, params: &LocalTextParams) -> ProviderResult<Arc<dyn TextModel>>;
    /// Decides intents with the chat model `model_id`, the same one that polishes.
    async fn router(&self, model_id: &str) -> ProviderResult<Arc<dyn Router>>;
    fn status(&self, model_id: &str) -> EngineStatus;
}

/// What the pipeline asks for engines. Tests and `--mock` runs substitute [`FixedEngines`].
#[async_trait]
pub trait Engines: Send + Sync {
    async fn transcriber(&self, cfg: &Config) -> ProviderResult<Arc<dyn Transcriber>>;
    /// `None` when polishing is switched off.
    async fn text_model(&self, cfg: &Config) -> ProviderResult<Option<Arc<dyn TextModel>>>;
    /// The text model Dito is set to, whether or not dictation polishes with it: commands and the Local API use it
    /// with the polish switch off.
    async fn configured_text_model(&self, cfg: &Config) -> ProviderResult<Arc<dyn TextModel>> {
        let mut cfg = cfg.clone();
        cfg.polish.enabled = true;
        self.text_model(&cfg)
            .await?
            .ok_or_else(|| ProviderError::new(ErrorKind::Unavailable, "no text model is available"))
    }
    /// Command mode routes with a local chat model whatever the polish source is: the instruction never leaves the Mac.
    async fn router(&self, cfg: &Config) -> ProviderResult<Arc<dyn Router>>;
    /// The configured text model's local status. `None` means the implementation has no local status to inspect.
    fn text_model_status(&self, _cfg: &Config) -> Option<EngineStatus> {
        None
    }
    /// Gets the cloud engines `cfg` selects ready for a run that will ask for them shortly: each one is built and its
    /// connection opened. Local engines are left alone, because asking for one loads its model. Nothing is reported:
    /// an engine that cannot be built fails the run that asks for it.
    async fn warm_up(&self, _cfg: &Config) {}
}

/// How many cloud engines of one kind stay built: the five contenders of a speech comparison fit beside the one
/// dictation uses, and text needs two, polish and chat. The bound is there for configurations that were edited away.
const CACHED_ENGINES: usize = 8;

/// Everything a cloud text engine is built from, except the secret stored under `account` and the factory's own
/// `bedrock_endpoint`, which never changes. A configuration's name is not here, and neither is the rest of the list.
#[derive(Clone, PartialEq)]
struct TextInputs {
    /// `None` for Bedrock, which authenticates through its AWS profile.
    account: Option<String>,
    target: CloudTextTarget,
    params: CloudTextParams,
}

impl TextInputs {
    fn of(configuration: &CloudTextConfiguration) -> Self {
        let account = match configuration.target {
            CloudTextTarget::Azure { .. } | CloudTextTarget::Compat { .. } => Some(configuration.account()),
            CloudTextTarget::Bedrock { .. } => None,
        };
        Self { account, target: configuration.target.clone(), params: configuration.params.clone() }
    }
}

/// Everything a cloud speech engine is built from, except the secret stored under `account`.
#[derive(Clone, PartialEq)]
struct SpeechInputs {
    /// `None` for the `with_speech_config` hook, which sends no key.
    account: Option<String>,
    config: serde_json::Value,
}

/// The engines used most recently, the latest last. An engine dropped from here lives on with whoever still holds it.
struct Recent<K, T: ?Sized>(Vec<(K, Arc<T>)>);

impl<K, T: ?Sized> Default for Recent<K, T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<K: PartialEq, T: ?Sized> Recent<K, T> {
    fn get(&mut self, inputs: &K) -> Option<Arc<T>> {
        let index = self.0.iter().position(|(built_from, _)| built_from == inputs)?;
        let entry = self.0.remove(index);
        let engine = entry.1.clone();
        self.0.push(entry);
        Some(engine)
    }

    fn put(&mut self, inputs: K, engine: Arc<T>) {
        self.0.retain(|(built_from, _)| *built_from != inputs);
        if self.0.len() == CACHED_ENGINES {
            self.0.remove(0);
        }
        self.0.push((inputs, engine));
    }
}

/// Cloud engines shared by every caller, so HTTP connections, cloud credentials and what an engine learned about its
/// server survive between requests. The API key is not part of what an engine is looked up by; `invalidate_engines`
/// must run whenever a secret changes.
#[derive(Default)]
struct EngineCache {
    /// Bumped by every invalidation so a build that raced with one is not stored.
    epoch: u64,
    speech: Recent<SpeechInputs, dyn Transcriber>,
    text: Recent<TextInputs, dyn TextModel>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lookup {
    Reuse,
    /// Build anew and keep the result in place of the cached engine.
    Rebuild,
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
        cache.speech = Recent::default();
        cache.text = Recent::default();
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

    fn secret(&self, account: Option<&str>) -> String {
        account.and_then(|account| self.secrets.get(account)).unwrap_or_default()
    }

    /// Takes nothing but the inputs the engine is cached under, so nothing outside them can shape it.
    async fn build_text(&self, inputs: &TextInputs) -> ProviderResult<Arc<dyn TextModel>> {
        let key = || self.secret(inputs.account.as_deref());
        let params = inputs.params.clone();
        Ok(match &inputs.target {
            CloudTextTarget::Azure { endpoint, deployment } => Arc::new(OpenAiEngine::new(
                Flavor::Azure { endpoint: endpoint.clone(), deployment: deployment.clone() },
                key(),
                params,
            )?),
            CloudTextTarget::Compat { base_url, model } => Arc::new(OpenAiEngine::new(
                Flavor::Compat { base_url: base_url.clone(), model: model.clone() },
                key(),
                params,
            )?),
            CloudTextTarget::Bedrock { profile, region, model_id } => Arc::new(
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

    fn speech_inputs(&self, e: &EngineConfig) -> ProviderResult<SpeechInputs> {
        if let Some(config) = &self.speech_config {
            return Ok(SpeechInputs { account: None, config: config.clone() });
        }
        // `cloud_asr` stays `None` until a configuration list is first saved; the bundled configurations stand in.
        let bundled;
        let configurations = match &e.cloud_asr {
            Some(configurations) => configurations,
            None => {
                bundled = CloudAsrConfigurations::default();
                &bundled
            }
        };
        let active = configurations
            .active()
            .ok_or_else(|| ProviderError::new(ErrorKind::Unavailable, "no cloud speech configuration is selected"))?;
        Ok(SpeechInputs { account: Some(active.account()), config: active.config.clone() })
    }

    fn require_local(&self) -> ProviderResult<&Arc<dyn LocalBackend>> {
        self.local
            .as_ref()
            .ok_or_else(|| ProviderError::new(ErrorKind::Unavailable, "local engines are not available in this build"))
    }

    /// The one way to a cloud engine. `build` reads the secret, so it must not start before the epoch is read: the
    /// lock is not held while it runs, and an engine whose build an invalidation overtook is returned but not kept.
    async fn cloud_engine<K: Clone + PartialEq, T: ?Sized>(
        &self,
        kind: fn(&mut EngineCache) -> &mut Recent<K, T>,
        inputs: &K,
        lookup: Lookup,
        build: impl Future<Output = ProviderResult<Arc<T>>>,
    ) -> ProviderResult<Arc<T>> {
        let epoch = {
            let mut cache = self.lock_cache();
            if lookup == Lookup::Reuse
                && let Some(engine) = kind(&mut cache).get(inputs)
            {
                return Ok(engine);
            }
            cache.epoch
        };
        let engine = build.await?;
        let mut cache = self.lock_cache();
        if cache.epoch == epoch {
            kind(&mut cache).put(inputs.clone(), engine.clone());
        }
        Ok(engine)
    }

    async fn speech_engine(&self, e: &EngineConfig, lookup: Lookup) -> ProviderResult<Arc<dyn Transcriber>> {
        match e.source {
            Source::Local => self.require_local()?.transcriber(&e.local_model).await,
            Source::Provider => {
                let inputs = self.speech_inputs(e)?;
                let build = async {
                    let key = self.secret(inputs.account.as_deref());
                    Ok(Arc::new(ConfiguredAsr::new(inputs.config.clone(), key)?) as Arc<dyn Transcriber>)
                };
                self.cloud_engine(|cache| &mut cache.speech, &inputs, lookup, build).await
            }
        }
    }

    async fn text_engine(&self, e: &EngineConfig, lookup: Lookup) -> ProviderResult<Arc<dyn TextModel>> {
        match e.source {
            // Local engines are not cached here, so a changed parameter applies to the next dictation.
            Source::Local => {
                self.require_local()?.text_model(&e.local_model, &e.local_params_for(&e.local_model)).await
            }
            Source::Provider => {
                // `cloud_text` stays `None` until a list is first saved; the templates stand in.
                let active = e.active_text().ok_or_else(|| {
                    ProviderError::new(ErrorKind::Unavailable, "no cloud polish configuration is selected")
                })?;
                let inputs = TextInputs::of(&active);
                self.cloud_engine(|cache| &mut cache.text, &inputs, lookup, self.build_text(&inputs)).await
            }
        }
    }

    /// The speech engine `e` selects. A cloud engine is the one every caller of that configuration shares.
    pub async fn transcriber_for(&self, e: &EngineConfig) -> ProviderResult<Arc<dyn Transcriber>> {
        self.speech_engine(e, Lookup::Reuse).await
    }

    /// The text engine `e` selects. A cloud engine is the one every caller of that configuration shares.
    pub async fn text_model_for(&self, e: &EngineConfig) -> ProviderResult<Arc<dyn TextModel>> {
        self.text_engine(e, Lookup::Reuse).await
    }

    /// A cloud engine built anew, which then replaces the shared one. For callers that must start from nothing: the
    /// connection test, and a comparison, which times every contender from a new connection.
    pub async fn rebuild_transcriber_for(&self, e: &EngineConfig) -> ProviderResult<Arc<dyn Transcriber>> {
        self.speech_engine(e, Lookup::Rebuild).await
    }

    /// As `rebuild_transcriber_for`. A new Bedrock engine also reads the AWS profile again, which a built one does not
    /// do for credentials written in the profile files.
    pub async fn rebuild_text_model_for(&self, e: &EngineConfig) -> ProviderResult<Arc<dyn TextModel>> {
        self.text_engine(e, Lookup::Rebuild).await
    }
}

#[async_trait]
impl Engines for EngineFactory {
    async fn transcriber(&self, cfg: &Config) -> ProviderResult<Arc<dyn Transcriber>> {
        self.transcriber_for(&cfg.asr).await
    }

    async fn text_model(&self, cfg: &Config) -> ProviderResult<Option<Arc<dyn TextModel>>> {
        if !cfg.polish.enabled {
            return Ok(None);
        }
        self.text_model_for(&cfg.text_model).await.map(Some)
    }

    async fn router(&self, cfg: &Config) -> ProviderResult<Arc<dyn Router>> {
        if let Some(router) = &self.router {
            return Ok(router.clone());
        }
        self.require_local()?.router(&cfg.text_model.local_model).await
    }

    fn text_model_status(&self, cfg: &Config) -> Option<EngineStatus> {
        match cfg.text_model.source {
            Source::Local => self.local.as_deref().map(|local| local.status(&cfg.text_model.local_model)),
            Source::Provider => Some(EngineStatus::Remote),
        }
    }

    async fn warm_up(&self, cfg: &Config) {
        let speech = async {
            if cfg.asr.source == Source::Provider
                && let Ok(engine) = self.transcriber_for(&cfg.asr).await
            {
                engine.preconnect().await;
            }
        };
        let text = async {
            if cfg.polish.enabled
                && cfg.text_model.source == Source::Provider
                && let Ok(engine) = self.text_model_for(&cfg.text_model).await
            {
                engine.preconnect().await;
            }
        };
        tokio::join!(speech, text);
    }
}

pub struct FixedEngines {
    pub asr: Arc<dyn Transcriber>,
    pub polish: Arc<dyn TextModel>,
    pub router: Arc<dyn Router>,
}

impl FixedEngines {
    /// Routes every instruction to polish, confidently.
    pub fn new(asr: Arc<dyn Transcriber>, polish: Arc<dyn TextModel>) -> Self {
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
            Arc::new(MockTextModel::default().with_delay(std::time::Duration::from_millis(700))),
        )
    }
}

#[async_trait]
impl Engines for FixedEngines {
    async fn transcriber(&self, _: &Config) -> ProviderResult<Arc<dyn Transcriber>> {
        Ok(self.asr.clone())
    }

    async fn text_model(&self, cfg: &Config) -> ProviderResult<Option<Arc<dyn TextModel>>> {
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
        let engine = &mut cfg.text_model;
        engine.source = Source::Provider;
        let configurations = engine.text_configurations_mut(lang);
        configurations.select("compat");
        let compat = configurations.active_mut().expect("the templates include a compat configuration");
        *compat.target.address_mut() = "http://127.0.0.1:1/v1".into();
        *compat.target.model_mut() = model.into();
        cfg
    }

    fn with_cloud_speech(mut cfg: Config) -> Config {
        cfg.asr.source = Source::Provider;
        let configurations = cfg.asr.cloud_asr.insert(CloudAsrConfigurations::default());
        configurations.active_mut().expect("the first template is selected")["connection"]["model"] = "speech-a".into();
        cfg
    }

    async fn text_model(factory: &EngineFactory, cfg: &Config) -> Arc<dyn TextModel> {
        factory.text_model(cfg).await.expect("compat engine builds offline").expect("polish is enabled")
    }

    #[tokio::test]
    async fn every_caller_of_one_cloud_configuration_shares_its_engine() {
        let factory = factory();
        let cfg = with_cloud_speech(compat_config("model-a"));
        let first = text_model(&factory, &cfg).await;
        assert!(Arc::ptr_eq(&first, &text_model(&factory, &cfg).await));
        let direct = factory.text_model_for(&cfg.text_model).await.unwrap();
        assert!(Arc::ptr_eq(&first, &direct), "dictation and the callers of text_model_for");
        assert!(Arc::ptr_eq(&direct, &factory.text_model_for(&cfg.text_model).await.unwrap()));

        let speech = factory.transcriber(&cfg).await.expect("a speech configuration with a model builds offline");
        assert!(Arc::ptr_eq(&speech, &factory.transcriber_for(&cfg.asr).await.unwrap()));
    }

    #[tokio::test]
    async fn a_changed_model_or_address_builds_a_new_engine_and_the_earlier_one_is_kept() {
        let factory = factory();
        let first = text_model(&factory, &compat_config("model-a")).await;
        let second = text_model(&factory, &compat_config("model-b")).await;
        assert!(!Arc::ptr_eq(&first, &second));
        assert!(Arc::ptr_eq(&second, &text_model(&factory, &compat_config("model-b")).await));

        let mut moved = compat_config("model-a");
        *moved.text_model.text_configurations_mut(moved.general.lang()).active_mut().unwrap().target.address_mut() =
            "http://127.0.0.1:2/v1".into();
        assert!(!Arc::ptr_eq(&first, &text_model(&factory, &moved).await), "the address is part of the engine");
        assert!(Arc::ptr_eq(&first, &text_model(&factory, &compat_config("model-a")).await));
    }

    #[tokio::test]
    async fn what_the_engine_in_use_is_not_built_from_can_change_without_replacing_it() {
        let factory = factory();
        let mut cfg = compat_config("model-a");
        let first = text_model(&factory, &cfg).await;

        cfg.text_model.local_model = "another-model".into();
        cfg.text_model.set_local_params("another-model", LocalTextParams { top_k: Some(20), ..Default::default() });
        let configurations = cfg.text_model.text_configurations_mut(cfg.general.lang());
        configurations.active_mut().unwrap().name = "Renamed".into();
        let inactive = configurations.get_mut("azure").expect("the templates include an Azure configuration");
        inactive.name = "Edited".into();
        *inactive.target.address_mut() = "https://example.invalid".into();
        inactive.params.temperature = Some(0.9);
        assert!(Arc::ptr_eq(&first, &text_model(&factory, &cfg).await));
    }

    #[tokio::test]
    async fn the_least_recently_used_engine_makes_room_once_the_cache_is_full() {
        let factory = factory();
        let model = |index: usize| compat_config(&format!("model-{index}"));
        let kept = text_model(&factory, &model(0)).await;
        let dropped = text_model(&factory, &model(1)).await;
        for index in 2..CACHED_ENGINES {
            text_model(&factory, &model(index)).await;
        }
        assert!(Arc::ptr_eq(&kept, &text_model(&factory, &model(0)).await), "a full cache still serves its engines");
        text_model(&factory, &model(CACHED_ENGINES)).await;
        assert!(Arc::ptr_eq(&kept, &text_model(&factory, &model(0)).await), "used after the second, so it stays");
        assert!(!Arc::ptr_eq(&dropped, &text_model(&factory, &model(1)).await));
    }

    #[tokio::test]
    async fn a_changed_parameter_or_selection_builds_a_new_engine() {
        let factory = factory();
        let base = compat_config("model-a");
        let first = text_model(&factory, &base).await;

        let lang = base.general.lang();
        let mut warmer = base.clone();
        warmer.text_model.text_configurations_mut(lang).active_mut().unwrap().params.temperature = Some(0.9);
        let second = text_model(&factory, &warmer).await;
        assert!(!Arc::ptr_eq(&first, &second), "a parameter is part of the engine");

        let mut other = warmer.clone();
        let copy = other.text_model.text_configurations_mut(lang).duplicate("compat", "Copy").unwrap();
        assert_eq!(other.text_model.text_configurations(lang).selected, copy);
        let third = text_model(&factory, &other).await;
        assert!(!Arc::ptr_eq(&second, &third), "another configuration is another engine");
    }

    #[derive(Default)]
    struct Recorder(Mutex<Vec<(String, LocalTextParams)>>);

    #[async_trait]
    impl LocalBackend for Recorder {
        async fn transcriber(&self, _: &str) -> ProviderResult<Arc<dyn Transcriber>> {
            Err(ProviderError::new(ErrorKind::Unavailable, "speech is not under test"))
        }
        async fn text_model(&self, model_id: &str, params: &LocalTextParams) -> ProviderResult<Arc<dyn TextModel>> {
            self.0.lock().unwrap().push((model_id.into(), *params));
            Ok(Arc::new(MockTextModel::default()))
        }
        async fn router(&self, _: &str) -> ProviderResult<Arc<dyn Router>> {
            Err(ProviderError::new(ErrorKind::Unavailable, "routing is not under test"))
        }
        fn status(&self, _: &str) -> EngineStatus {
            EngineStatus::Running
        }
    }

    #[tokio::test]
    async fn a_local_text_model_gets_the_parameters_saved_for_its_model_and_for_no_other() {
        let backend = Arc::new(Recorder::default());
        let factory = factory().with_local(backend.clone());
        let mut cfg = Config::default();
        let model = cfg.text_model.local_model.clone();
        let params = LocalTextParams { temperature: Some(0.5), top_k: Some(20), ..LocalTextParams::default() };
        cfg.text_model.set_local_params(&model, params);
        cfg.text_model.set_local_params("another-model", LocalTextParams { max_tokens: Some(9), ..Default::default() });

        factory.text_model(&cfg).await.unwrap().expect("polish is enabled");
        cfg.text_model.set_local_params(&model, LocalTextParams::default());
        factory.text_model(&cfg).await.unwrap().expect("polish is enabled");

        let asked = backend.0.lock().unwrap().clone();
        assert_eq!(
            asked,
            [(model.clone(), params), (model, LocalTextParams::default())],
            "nothing is cached for local"
        );
    }

    #[tokio::test]
    async fn a_selection_that_points_nowhere_is_a_reported_problem_not_a_default() {
        let mut cfg = compat_config("model-a");
        cfg.text_model.text_configurations_mut(cfg.general.lang()).selected = "gone".into();
        let error = factory().text_model(&cfg).await.err().expect("nothing is selected");
        assert_eq!(error.kind, ErrorKind::Unavailable);
    }

    #[tokio::test]
    async fn invalidating_drops_every_cached_engine() {
        let factory = factory();
        let (a, b) = (with_cloud_speech(compat_config("model-a")), compat_config("model-b"));
        let first = (text_model(&factory, &a).await, text_model(&factory, &b).await);
        let speech = factory.transcriber(&a).await.unwrap();
        factory.invalidate_engines();
        assert!(!Arc::ptr_eq(&first.0, &text_model(&factory, &a).await));
        assert!(!Arc::ptr_eq(&first.1, &text_model(&factory, &b).await));
        assert!(!Arc::ptr_eq(&speech, &factory.transcriber(&a).await.unwrap()));
    }

    #[tokio::test]
    async fn a_rebuilt_engine_replaces_the_shared_one() {
        let factory = factory();
        let cfg = compat_config("model-a");
        let first = text_model(&factory, &cfg).await;
        let rebuilt = factory.rebuild_text_model_for(&cfg.text_model).await.unwrap();
        assert!(!Arc::ptr_eq(&first, &rebuilt));
        assert!(Arc::ptr_eq(&rebuilt, &text_model(&factory, &cfg).await));
    }

    #[tokio::test]
    async fn an_engine_whose_build_an_invalidation_overtook_is_not_kept() {
        let factory = factory();
        let cfg = compat_config("model-a");
        let inputs = TextInputs::of(&cfg.text_model.active_text().unwrap());
        let overtaken = async {
            factory.invalidate_engines();
            factory.build_text(&inputs).await
        };
        let stale = factory.cloud_engine(|cache| &mut cache.text, &inputs, Lookup::Reuse, overtaken).await.unwrap();
        assert!(!Arc::ptr_eq(&stale, &text_model(&factory, &cfg).await));
    }
}
