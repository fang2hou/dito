use std::sync::Arc;

use axum::Router;
use axum::routing;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::behavior::Behavior;
use crate::recorded::Recorded;
use crate::shared::Shared;
use crate::{bedrock, files, openai};

pub struct MockServer {
    pub url: String,
    shared: Arc<Shared>,
    task: JoinHandle<()>,
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl MockServer {
    pub async fn start() -> anyhow::Result<Self> {
        Self::start_on("127.0.0.1:0").await
    }

    pub async fn start_on(addr: &str) -> anyhow::Result<Self> {
        let shared = Arc::new(Shared::default());
        let app = Router::new()
            .route("/openai/v1/audio/transcriptions", routing::post(openai::transcribe))
            .route("/v1/audio/transcriptions", routing::post(openai::transcribe))
            .route("/openai/v1/chat/completions", routing::post(openai::chat))
            .route("/v1/chat/completions", routing::post(openai::chat))
            .route("/model/{model}/converse", routing::post(bedrock::converse))
            .route("/files/{name}", routing::get(files::download))
            .with_state(shared.clone());
        let listener = TcpListener::bind(addr).await?;
        let url = format!("http://{}", listener.local_addr()?);
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Ok(Self { url, shared, task })
    }

    pub fn behavior(&self, f: impl FnOnce(&mut Behavior)) {
        self.shared.update_behavior(f);
    }

    pub fn add_file(&self, name: &str, bytes: Vec<u8>) {
        self.shared.add_file(name, bytes);
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.shared.requests()
    }
}
