//! Turns the current [`Config`] into concrete engines.

use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;

use crate::config::{AsrProfiles, Config, EngineConfig, ProviderKind, Source};
use crate::error::{ErrorKind, ProviderError, ProviderResult};
use crate::health::EngineStatus;
use crate::provider::bedrock::{BedrockEngine, BedrockParams};
use crate::provider::configured_asr::ConfiguredAsr;
use crate::provider::mock::{MockPolisher, MockTranscriber};
use crate::provider::openai::{Flavor, OpenAiEngine};
use crate::provider::{Polisher, Transcriber};
use crate::secrets::{Role, SecretStore, account};

/// On-device engines, implemented by `dito-local`.
#[async_trait]
pub trait LocalBackend: Send + Sync {
    async fn transcriber(&self, model_id: &str) -> ProviderResult<Arc<dyn Transcriber>>;
    async fn polisher(&self, model_id: &str) -> ProviderResult<Arc<dyn Polisher>>;
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
    cache: Mutex<EngineCache>,
}

impl EngineFactory {
    pub fn new(secrets: Arc<dyn SecretStore>) -> Self {
        Self { secrets, local: None, bedrock_endpoint: None, cache: Mutex::default() }
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

    fn polish_key(&self, kind: ProviderKind) -> String {
        self.secrets.get(&account(Role::Polish, kind)).unwrap_or_default()
    }

    fn openai(&self, e: &EngineConfig) -> ProviderResult<OpenAiEngine> {
        let p = &e.provider;
        match p.kind {
            ProviderKind::Azure => OpenAiEngine::new(
                Flavor::Azure { endpoint: p.azure.endpoint.clone(), deployment: p.azure.deployment.clone() },
                self.polish_key(ProviderKind::Azure),
                p.azure.reasoning_effort.clone(),
            ),
            ProviderKind::Compat => OpenAiEngine::new(
                Flavor::Compat { base_url: p.compat.base_url.clone(), model: p.compat.model.clone() },
                self.polish_key(ProviderKind::Compat),
                p.compat.reasoning_effort.clone(),
            ),
            ProviderKind::Bedrock => unreachable!("handled by caller"),
        }
    }

    async fn bedrock(&self, e: &EngineConfig) -> ProviderResult<BedrockEngine> {
        let b = &e.provider.bedrock;
        BedrockEngine::connect(BedrockParams {
            profile: b.profile.clone(),
            region: b.region.clone(),
            model_id: b.model_id.clone(),
            endpoint_override: self.bedrock_endpoint.clone(),
        })
        .await
        .map(|engine| engine.with_reasoning_effort(b.reasoning_effort.as_str()))
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
        match (e.source, e.provider.kind) {
            (Source::Local, _) => self.require_local()?.polisher(&e.local_model).await,
            (Source::Provider, ProviderKind::Bedrock) => Ok(Arc::new(self.bedrock(e).await?)),
            (Source::Provider, _) => Ok(Arc::new(self.openai(e)?)),
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
        engine.provider.kind = ProviderKind::Compat;
        engine.provider.compat.base_url = "http://127.0.0.1:1/v1".into();
        engine.provider.compat.model = model.into();
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
    async fn invalidating_forces_a_rebuild() {
        let factory = factory();
        let cfg = compat_config("model-a");
        let first = polisher(&factory, &cfg).await;
        factory.invalidate_engines();
        let second = polisher(&factory, &cfg).await;
        assert!(!Arc::ptr_eq(&first, &second));
    }
}
