//! Turns the current [`Config`] into concrete engines.

use std::sync::Arc;

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

pub struct EngineFactory {
    secrets: Arc<dyn SecretStore>,
    local: Option<Arc<dyn LocalBackend>>,
    bedrock_endpoint: Option<String>,
}

impl EngineFactory {
    pub fn new(secrets: Arc<dyn SecretStore>) -> Self {
        Self { secrets, local: None, bedrock_endpoint: None }
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
        self.transcriber_for(&cfg.asr).await
    }

    async fn polisher(&self, cfg: &Config) -> ProviderResult<Option<Arc<dyn Polisher>>> {
        if !cfg.polish.enabled {
            return Ok(None);
        }
        Ok(Some(self.polisher_for(&cfg.polish.engine).await?))
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
