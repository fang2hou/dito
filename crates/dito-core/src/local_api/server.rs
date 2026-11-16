//! The router: the Host and Origin guard around everything, token checks per surface, and the `/v1` handlers.

use std::convert::Infallible;
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Multipart, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures_util::stream;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::auth::{bearer, has_browser_origin, host_allowed, token_matches};
use super::models::{InstalledModels, served_models, thinking_for};
use super::openai::{self, ApiError, ChatCompletionRequest, Chunk};
use super::token::Token;
use super::wav::{self, WavError};
use super::{Backend, mcp};
use crate::config::{Config, ConfigStore};
use crate::error::ErrorKind;
use crate::provider::{AsrRequest, ChatCancel, ChatRequest};

const JSON_LIMIT: usize = 16 * 1024 * 1024;
const UPLOAD_LIMIT: usize = 256 * 1024 * 1024;
const ABORT_AFTER: std::time::Duration = std::time::Duration::from_secs(3);

pub(super) struct Shared {
    pub(super) cfg: Arc<ConfigStore>,
    pub(super) backend: Arc<dyn Backend>,
    pub(super) installed: Arc<dyn InstalledModels>,
    pub(super) token: Token,
}

#[derive(Clone)]
struct ServerState {
    shared: Arc<Shared>,
    port: u16,
}

pub(super) fn router(shared: Arc<Shared>, port: u16, cancel: CancellationToken) -> Router {
    let state = ServerState { shared: shared.clone(), port };
    let api = Router::new()
        .route("/models", get(models))
        .route("/chat/completions", post(chat_completions).layer(DefaultBodyLimit::max(JSON_LIMIT)))
        .route("/audio/transcriptions", post(transcriptions).layer(DefaultBodyLimit::max(UPLOAD_LIMIT)))
        .layer(middleware::from_fn_with_state(state.clone(), |s, r, n| {
            authorize(s, r, n, |c| c.local_api.api_any_token)
        }));
    let mcp = Router::new()
        .nest_service("/mcp", mcp::service(shared, cancel))
        .layer(middleware::from_fn_with_state(state.clone(), |s, r, n| {
            authorize(s, r, n, |c| c.local_api.mcp_any_token)
        }));
    Router::new()
        .nest("/v1", api)
        .merge(mcp)
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
}

/// Runs before everything else, so a page on another site cannot reach any route, `/mcp` included.
async fn guard(State(state): State<ServerState>, request: Request, next: Next) -> Response {
    let host = request.headers().get(header::HOST).and_then(|v| v.to_str().ok());
    if !host_allowed(host, state.port) || has_browser_origin(request.headers()) {
        return ApiError::forbidden().into_response();
    }
    next.run(request).await
}

async fn authorize(
    State(state): State<ServerState>,
    request: Request,
    next: Next,
    any_token: fn(&Config) -> bool,
) -> Response {
    if !any_token(&state.shared.cfg.get()) {
        let presented = bearer(request.headers()).unwrap_or_default();
        if !token_matches(&state.shared.token.get(), presented) {
            return ApiError::unauthorized().into_response();
        }
    }
    next.run(request).await
}

async fn models(State(state): State<ServerState>) -> impl IntoResponse {
    let data: Vec<_> = served_models(&state.shared.cfg.get())
        .iter()
        .map(|m| json!({ "id": m.id, "object": "model", "created": 0, "owned_by": "dito", "role": m.role }))
        .collect();
    axum::Json(json!({ "object": "list", "data": data }))
}

/// One reply from the text model Dito is set to, whatever the polish switch says.
pub(super) async fn generate(shared: &Shared, request: &ChatRequest, cancel: &ChatCancel) -> Result<String, ApiError> {
    let model = shared.backend.text_model(&shared.cfg.get()).await.map_err(|e| ApiError::from(&e))?;
    model.chat(request, &mut |_: &str| {}, cancel).await.map_err(|e| ApiError::from(&e))
}

/// Cancels the reply when the response is dropped, which is what a client closing the connection does.
struct CancelOnDrop {
    cancel: ChatCancel,
    task: Option<tokio::task::AbortHandle>,
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.cancel.cancel();
        // The engine stops at its next token; the abort is for one that never checks, such as a cloud request.
        if let Some(task) = self.task.take() {
            tokio::spawn(async move {
                tokio::time::sleep(ABORT_AFTER).await;
                task.abort();
            });
        }
    }
}

enum Piece {
    Text(String),
    Finished,
    Failed(ApiError),
}

async fn chat_completions(State(state): State<ServerState>, body: Bytes) -> Result<Response, ApiError> {
    let request: ChatCompletionRequest = serde_json::from_slice(&body)
        .map_err(|e| ApiError::invalid("invalid_json", format!("The body is not a chat completion request: {e}")))?;
    let stream = request.stream;
    let shared = state.shared;
    let thinking = thinking_for(request.reasoning_effort.as_deref(), &shared.cfg.get(), shared.installed.as_ref())
        .map_err(|supported| ApiError::unsupported_effort(&supported))?;
    let mut chat = request.into_chat()?;
    chat.thinking = thinking;
    let model_id = served_models(&shared.cfg.get())[0].id.clone();
    let (id, created) = (openai::completion_id(), openai::unix_seconds());

    if !stream {
        let guard = CancelOnDrop { cancel: ChatCancel::default(), task: None };
        let reply = generate(&shared, &chat, &guard.cancel).await?;
        return Ok(axum::Json(openai::completion(&id, created, &model_id, &reply)).into_response());
    }

    let model = shared.backend.text_model(&shared.cfg.get()).await.map_err(|e| ApiError::from(&e))?;
    let cancel = ChatCancel::default();
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn({
        let cancel = cancel.clone();
        async move {
            let sender = tx.clone();
            let mut on_delta = move |piece: &str| {
                let _ = sender.send(Piece::Text(piece.to_owned()));
            };
            let end = match model.chat(&chat, &mut on_delta, &cancel).await {
                Ok(_) => Piece::Finished,
                Err(e) => Piece::Failed(ApiError::from(&e)),
            };
            let _ = tx.send(end);
        }
    });
    let hold = CancelOnDrop { cancel, task: Some(task.abort_handle()) };

    enum Phase {
        Opening,
        Body,
        Closing,
        Done,
    }
    let events = stream::unfold((Phase::Opening, rx, hold), move |(phase, mut rx, hold)| {
        let (id, model_id) = (id.clone(), model_id.clone());
        async move {
            let event = |value: serde_json::Value| Event::default().data(value.to_string());
            let (item, next) = match phase {
                Phase::Opening => (event(openai::chunk(&id, created, &model_id, Chunk::Role)), Phase::Body),
                Phase::Body => match rx.recv().await {
                    Some(Piece::Text(text)) => {
                        (event(openai::chunk(&id, created, &model_id, Chunk::Text(&text))), Phase::Body)
                    }
                    Some(Piece::Finished) => {
                        (event(openai::chunk(&id, created, &model_id, Chunk::Stop)), Phase::Closing)
                    }
                    Some(Piece::Failed(e)) => (event(e.body()), Phase::Closing),
                    None => (
                        event(
                            ApiError::from(&crate::error::ProviderError::new(ErrorKind::Other, "stream ended")).body(),
                        ),
                        Phase::Closing,
                    ),
                },
                Phase::Closing => (Event::default().data("[DONE]"), Phase::Done),
                Phase::Done => return None,
            };
            Some((Ok::<_, Infallible>(item), (next, rx, hold)))
        }
    });
    Ok(Sse::new(events).keep_alive(KeepAlive::default()).into_response())
}

async fn transcriptions(State(state): State<ServerState>, mut form: Multipart) -> Result<Response, ApiError> {
    let (mut file, mut language, mut format) = (None, None, String::from("json"));
    while let Some(field) =
        form.next_field().await.map_err(|e| ApiError::invalid("invalid_multipart", e.to_string()))?
    {
        let name = field.name().unwrap_or_default().to_owned();
        let too_big = |e: axum::extract::multipart::MultipartError| {
            let mut err = ApiError::invalid("invalid_multipart", e.to_string());
            err.status = e.status();
            err
        };
        match name.as_str() {
            "file" => file = Some(field.bytes().await.map_err(too_big)?),
            "language" => language = Some(field.text().await.map_err(too_big)?).filter(|l| !l.trim().is_empty()),
            "response_format" => format = field.text().await.map_err(too_big)?,
            _ => {}
        }
    }
    let Some(file) = file else {
        return Err(ApiError::invalid("missing_file", "The form needs a file field with the audio."));
    };
    if !matches!(format.as_str(), "json" | "text" | "verbose_json") {
        return Err(ApiError::invalid(
            "unsupported_response_format",
            "response_format must be json, text or verbose_json.",
        ));
    }
    let clip = wav::decode(&file).map_err(|e| ApiError {
        status: StatusCode::UNSUPPORTED_MEDIA_TYPE,
        kind: "invalid_request_error",
        code: "unsupported_media_type",
        message: match e {
            WavError::NotWav => "The file is not a WAV file. Convert the audio to WAV first.".into(),
            WavError::Unsupported(why) => why.into(),
        },
    })?;
    let shared = state.shared;
    let transcriber = shared.backend.transcriber(&shared.cfg.get()).await.map_err(|e| ApiError::from(&e))?;
    let request = AsrRequest { clip: &clip, language: language.as_deref(), hotwords: &[] };
    let transcript = match transcriber.transcribe(&request).await {
        Ok(transcript) => transcript,
        // Silence is an empty transcript, as it is for OpenAI, not a failure.
        Err(e) if e.kind == ErrorKind::NoSpeech => crate::provider::Transcript { text: String::new(), language: None },
        Err(e) => return Err(ApiError::from(&e)),
    };
    Ok(match format.as_str() {
        "text" => {
            let mut response = transcript.text.into_response();
            response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
            response
        }
        "verbose_json" => axum::Json(json!({
            "task": "transcribe",
            "language": transcript.language.or(language),
            "duration": clip.duration_ms() as f64 / 1000.,
            "text": transcript.text,
        }))
        .into_response(),
        _ => axum::Json(openai::Transcription { text: &transcript.text }).into_response(),
    })
}
