//! The Local API service (`design:tools/local-api`, `design:tools/mcp`): an opt-in HTTP server on the loopback address
//! that lends Dito's own text and speech models to other apps, through an OpenAI-compatible API and MCP.
//!
//! The service follows `ConfigStore`: it starts when `local_api.enabled` turns on, stops when it turns off and
//! restarts on another port, so the page only writes settings.

mod auth;
mod check;
mod mcp;
mod models;
mod openai;
mod server;
mod token;
mod wav;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

pub use check::{CheckFailure, Surface, check_connection};
pub use mcp::{GENERATE_TEXT, LIST_LOCAL_MODELS, MODELS_RESOURCE, tool_schemas};
pub use models::{
    InstalledModel, InstalledModels, ModelRole, SPEECH_MODEL_ID, ServedModel, TEXT_MODEL_ID, served_models,
};
use server::Shared;
use token::Token;

use crate::config::{Config, ConfigStore};
use crate::error::{ErrorKind, ProviderError, ProviderResult};
use crate::factory::{EngineFactory, Engines};
use crate::provider::{TextModel, Transcriber};
use crate::secrets::{SecretError, SecretStore};

const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

/// The engines a request runs on: the text and speech model Dito is set to.
#[async_trait]
pub trait Backend: Send + Sync {
    async fn text_model(&self, cfg: &Config) -> ProviderResult<Arc<dyn TextModel>>;
    async fn transcriber(&self, cfg: &Config) -> ProviderResult<Arc<dyn Transcriber>>;
}

#[async_trait]
impl Backend for EngineFactory {
    /// The text model is the one Dito is set to whether or not dictation polishes with it.
    async fn text_model(&self, cfg: &Config) -> ProviderResult<Arc<dyn TextModel>> {
        let mut cfg = cfg.clone();
        cfg.polish.enabled = true;
        Engines::text_model(self, &cfg)
            .await?
            .ok_or_else(|| ProviderError::new(ErrorKind::Unavailable, "no text model is available"))
    }

    async fn transcriber(&self, cfg: &Config) -> ProviderResult<Arc<dyn Transcriber>> {
        Engines::transcriber(self, cfg).await
    }
}

pub struct Deps {
    pub cfg: Arc<ConfigStore>,
    pub secrets: Arc<dyn SecretStore>,
    pub backend: Arc<dyn Backend>,
    pub installed: Arc<dyn InstalledModels>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Stopped,
    Running {
        port: u16,
    },
    /// The port could not be bound, usually because another app has it. The next settings change tries again.
    Failed {
        port: u16,
        reason: String,
    },
}

/// The running service as the app sees it: the token and the state of the listener.
#[derive(Clone)]
pub struct LocalApi {
    shared: Arc<Shared>,
    status: watch::Receiver<Status>,
}

impl LocalApi {
    pub fn spawn(deps: Deps, rt: &tokio::runtime::Handle) -> Self {
        let shared = Arc::new(Shared {
            cfg: deps.cfg,
            backend: deps.backend,
            installed: deps.installed,
            token: Token::load(deps.secrets),
        });
        let (tx, status) = watch::channel(Status::Stopped);
        rt.spawn(supervise(shared.clone(), tx));
        Self { shared, status }
    }

    pub fn token(&self) -> String {
        self.shared.token.get()
    }

    /// An empty token is ignored: it would let anything in.
    pub fn set_token(&self, token: &str) -> Result<(), SecretError> {
        self.shared.token.set(token)
    }

    pub fn regenerate_token(&self) -> Result<String, SecretError> {
        self.shared.token.regenerate()
    }

    pub fn status(&self) -> watch::Receiver<Status> {
        self.status.clone()
    }
}

struct Listener {
    port: u16,
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl Listener {
    async fn stop(self) {
        self.cancel.cancel();
        let mut task = self.task;
        if tokio::time::timeout(SHUTDOWN_GRACE, &mut task).await.is_err() {
            // A long stream may outlive the grace period; the port must be free for the next start.
            task.abort();
            let _ = task.await;
        }
    }
}

async fn start(shared: &Arc<Shared>, port: u16) -> Result<Listener, std::io::Error> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let cancel = CancellationToken::new();
    let app = server::router(shared.clone(), port, cancel.clone());
    let stop = cancel.clone();
    let task = tokio::spawn(async move {
        let served = axum::serve(listener, app).with_graceful_shutdown(async move { stop.cancelled_owned().await });
        if let Err(e) = served.await {
            tracing::warn!("local api server stopped: {e}");
        }
    });
    Ok(Listener { port, cancel, task })
}

async fn supervise(shared: Arc<Shared>, status: watch::Sender<Status>) {
    let mut config = shared.cfg.subscribe();
    let mut listener: Option<Listener> = None;
    loop {
        let wanted = {
            let cfg = config.borrow_and_update();
            cfg.local_api.enabled.then_some(cfg.local_api.port)
        };
        // A failed start leaves no listener, so any later settings change tries the port again.
        if wanted != listener.as_ref().map(|l| l.port) {
            if let Some(old) = listener.take() {
                old.stop().await;
            }
            status.send_replace(Status::Stopped);
            if let Some(port) = wanted {
                match start(&shared, port).await {
                    Ok(started) => {
                        tracing::info!("local api listening on 127.0.0.1:{port}");
                        status.send_replace(Status::Running { port });
                        listener = Some(started);
                    }
                    Err(e) => {
                        tracing::warn!("local api could not listen on 127.0.0.1:{port}: {e}");
                        status.send_replace(Status::Failed { port, reason: e.to_string() });
                    }
                }
            }
        }
        if config.changed().await.is_err() {
            break;
        }
    }
    if let Some(old) = listener {
        old.stop().await;
    }
}

#[cfg(test)]
mod tests;
