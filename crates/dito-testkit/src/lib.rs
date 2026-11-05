//! A local mock of the cloud endpoints Dito talks to: Azure OpenAI, OpenAI-compatible servers and
//! Bedrock Converse. Used by integration tests and by `dito-mock-server` for offline development.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Multipart, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use dito_core::provider::mock::rule_polish;
use serde_json::{Value, json};
use tokio::net::TcpListener;

/// What the server does next. Mutate through [`MockServer::behavior`].
#[derive(Debug, Clone)]
pub struct Behavior {
    /// Expected secret: `api-key` header (Azure) or `Authorization: Bearer` (compat). `None` = no auth.
    pub expected_key: Option<String>,
    pub latency: Duration,
    /// Fail every request with this status (e.g. 429, 500).
    pub fail_status: Option<u16>,
    /// Reject `reasoning_effort` like a non-reasoning model would.
    pub reject_reasoning_effort: bool,
    pub transcript: String,
    pub language: Option<String>,
    /// File downloads: cut the connection after this many body bytes (first matching request only).
    pub cut_download_after: Option<usize>,
    /// File downloads: pause between 16 KiB chunks.
    pub download_chunk_delay: Duration,
}

impl Default for Behavior {
    fn default() -> Self {
        Self {
            expected_key: None,
            latency: Duration::ZERO,
            fail_status: None,
            reject_reasoning_effort: false,
            transcript: "えーと 明日の3時に会議があります".into(),
            language: Some("ja".into()),
            cut_download_after: None,
            download_chunk_delay: Duration::ZERO,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Recorded {
    pub path: String,
    pub model: Option<String>,
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub audio_bytes: usize,
    pub system: Option<String>,
    pub user: Option<String>,
    pub reasoning_effort: Option<String>,
    pub headers: Vec<(String, String)>,
}

#[derive(Default)]
struct Shared {
    behavior: Mutex<Behavior>,
    requests: Mutex<Vec<Recorded>>,
    files: Mutex<HashMap<String, Vec<u8>>>,
}

pub struct MockServer {
    pub url: String,
    shared: Arc<Shared>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

type St = State<Arc<Shared>>;

impl MockServer {
    pub async fn start() -> anyhow::Result<Self> {
        Self::start_on("127.0.0.1:0").await
    }

    pub async fn start_on(addr: &str) -> anyhow::Result<Self> {
        let shared = Arc::new(Shared::default());
        let app = Router::new()
            .route("/openai/v1/audio/transcriptions", post(transcribe))
            .route("/v1/audio/transcriptions", post(transcribe))
            .route("/openai/v1/chat/completions", post(chat))
            .route("/v1/chat/completions", post(chat))
            .route("/model/{model}/converse", post(converse))
            .route("/files/{name}", get(download))
            .with_state(shared.clone());
        let listener = TcpListener::bind(addr).await?;
        let url = format!("http://{}", listener.local_addr()?);
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Ok(Self { url, shared, task })
    }

    pub fn behavior(&self, f: impl FnOnce(&mut Behavior)) {
        f(&mut self.shared.behavior.lock().unwrap());
    }

    /// Serve `bytes` at `{url}/files/{name}` with HTTP Range support.
    pub fn add_file(&self, name: &str, bytes: Vec<u8>) {
        self.shared.files.lock().unwrap().insert(name.into(), bytes);
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.shared.requests.lock().unwrap().clone()
    }
}

fn header_pairs(h: &HeaderMap) -> Vec<(String, String)> {
    h.iter().map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_string())).collect()
}

/// Common gate: latency, forced failure, and key check. Returns an error response to send, if any.
async fn gate(s: &Shared, h: &HeaderMap) -> Option<Response> {
    let b = s.behavior.lock().unwrap().clone();
    if !b.latency.is_zero() {
        tokio::time::sleep(b.latency).await;
    }
    if let Some(code) = b.fail_status {
        return Some(
            (StatusCode::from_u16(code).unwrap(), Json(json!({"error": {"message": "forced failure"}})))
                .into_response(),
        );
    }
    if let Some(key) = b.expected_key {
        let ok = h.get("api-key").and_then(|v| v.to_str().ok()) == Some(key.as_str())
            || h.get("authorization").and_then(|v| v.to_str().ok()) == Some(format!("Bearer {key}").as_str());
        if !ok {
            return Some(
                (StatusCode::UNAUTHORIZED, Json(json!({"error": {"message": "invalid key"}}))).into_response(),
            );
        }
    }
    None
}

async fn transcribe(State(s): St, headers: HeaderMap, mut mp: Multipart) -> Response {
    let mut rec =
        Recorded { path: "audio/transcriptions".into(), headers: header_pairs(&headers), ..Default::default() };
    while let Ok(Some(field)) = mp.next_field().await {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "file" => rec.audio_bytes = field.bytes().await.map(|b| b.len()).unwrap_or(0),
            "model" => rec.model = field.text().await.ok(),
            "language" => rec.language = field.text().await.ok(),
            "prompt" => rec.prompt = field.text().await.ok(),
            _ => {}
        }
    }
    s.requests.lock().unwrap().push(rec);
    if let Some(r) = gate(&s, &headers).await {
        return r;
    }
    let b = s.behavior.lock().unwrap().clone();
    Json(json!({ "text": b.transcript, "language": b.language })).into_response()
}

async fn chat(State(s): St, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    let msg = |role: &str| {
        body["messages"]
            .as_array()
            .and_then(|m| m.iter().find(|x| x["role"] == role))
            .and_then(|x| x["content"].as_str())
            .map(str::to_string)
    };
    s.requests.lock().unwrap().push(Recorded {
        path: "chat/completions".into(),
        model: body["model"].as_str().map(str::to_string),
        system: msg("system"),
        user: msg("user"),
        reasoning_effort: body["reasoning_effort"].as_str().map(str::to_string),
        headers: header_pairs(&headers),
        ..Default::default()
    });
    if let Some(r) = gate(&s, &headers).await {
        return r;
    }
    if s.behavior.lock().unwrap().reject_reasoning_effort && body.get("reasoning_effort").is_some() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": {"message": "Unsupported parameter: 'reasoning_effort' is not supported with this model."}})),
        )
            .into_response();
    }
    let polished = rule_polish(msg("user").as_deref().unwrap_or_default());
    Json(json!({
        "id": "chatcmpl-mock",
        "object": "chat.completion",
        "choices": [{ "index": 0, "finish_reason": "stop", "message": { "role": "assistant", "content": polished } }]
    }))
    .into_response()
}

async fn converse(State(s): St, headers: HeaderMap, Path(model): Path<String>, Json(body): Json<Value>) -> Response {
    let has_audio =
        body["messages"][0]["content"].as_array().is_some_and(|c| c.iter().any(|x| x.get("audio").is_some()));
    let user = body["messages"][0]["content"]
        .as_array()
        .and_then(|c| c.iter().find_map(|x| x["text"].as_str()))
        .map(str::to_string);
    s.requests.lock().unwrap().push(Recorded {
        path: "converse".into(),
        model: Some(model),
        system: body["system"][0]["text"].as_str().map(str::to_string),
        user,
        audio_bytes: usize::from(has_audio),
        headers: header_pairs(&headers),
        ..Default::default()
    });
    let b = s.behavior.lock().unwrap().clone();
    if !b.latency.is_zero() {
        tokio::time::sleep(b.latency).await;
    }
    if let Some(code) = b.fail_status {
        let (kind, status) = match code {
            403 | 401 => ("AccessDeniedException", 403),
            429 => ("ThrottlingException", 429),
            404 => ("ResourceNotFoundException", 404),
            other => ("InternalServerException", other),
        };
        let mut h = HeaderMap::new();
        h.insert("x-amzn-errortype", HeaderValue::from_static(kind_static(kind)));
        return (StatusCode::from_u16(status).unwrap(), h, Json(json!({"message": "forced failure"}))).into_response();
    }
    let text = if has_audio {
        b.transcript
    } else {
        let original = s.requests.lock().unwrap().last().and_then(|r| r.user.clone()).unwrap_or_default();
        rule_polish(&original)
    };
    Json(json!({
        "output": { "message": { "role": "assistant", "content": [{ "text": text }] } },
        "stopReason": "end_turn",
        "usage": { "inputTokens": 1, "outputTokens": 1, "totalTokens": 2 },
        "metrics": { "latencyMs": 1 }
    }))
    .into_response()
}

fn kind_static(kind: &str) -> &'static str {
    match kind {
        "AccessDeniedException" => "AccessDeniedException",
        "ThrottlingException" => "ThrottlingException",
        "ResourceNotFoundException" => "ResourceNotFoundException",
        _ => "InternalServerException",
    }
}

async fn download(State(s): St, headers: HeaderMap, Path(name): Path<String>) -> Response {
    let Some(data) = s.files.lock().unwrap().get(&name).cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let total = data.len();
    let range = headers
        .get("range")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("bytes="))
        .and_then(|v| v.split('-').next())
        .and_then(|v| v.parse::<usize>().ok());
    s.requests.lock().unwrap().push(Recorded {
        path: format!("files/{name}"),
        headers: header_pairs(&headers),
        ..Default::default()
    });
    let start = range.unwrap_or(0);
    if start > total {
        return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
    }
    let (cut, delay) = {
        let mut b = s.behavior.lock().unwrap();
        (b.cut_download_after.take(), b.download_chunk_delay)
    };
    let mut body = data[start..].to_vec();
    let truncated = cut.is_some_and(|c| c < body.len());
    if let Some(c) = cut {
        body.truncate(c);
    }
    let declared = total - start;
    let chunks: Vec<Result<axum::body::Bytes, std::io::Error>> =
        body.chunks(16 * 1024).map(|c| Ok(axum::body::Bytes::copy_from_slice(c))).collect();
    let stream = futures_stream(chunks, delay, truncated);
    let mut resp = Response::new(axum::body::Body::from_stream(stream));
    *resp.status_mut() = if range.is_some() { StatusCode::PARTIAL_CONTENT } else { StatusCode::OK };
    let h = resp.headers_mut();
    h.insert("content-length", HeaderValue::from_str(&declared.to_string()).unwrap());
    if range.is_some() {
        h.insert("content-range", HeaderValue::from_str(&format!("bytes {start}-{}/{total}", total - 1)).unwrap());
    }
    h.insert("accept-ranges", HeaderValue::from_static("bytes"));
    resp
}

/// Chunk stream with optional per-chunk delay; when `truncated`, ends with an I/O error so the client sees a broken download.
fn futures_stream(
    chunks: Vec<Result<axum::body::Bytes, std::io::Error>>,
    delay: Duration,
    truncated: bool,
) -> impl futures_core::Stream<Item = Result<axum::body::Bytes, std::io::Error>> {
    let mut items: Vec<_> = chunks;
    if truncated {
        items.push(Err(std::io::Error::new(std::io::ErrorKind::ConnectionReset, "cut")));
    }
    tokio_stream_iter(items, delay)
}

fn tokio_stream_iter<T: Send + 'static>(items: Vec<T>, delay: Duration) -> impl futures_core::Stream<Item = T> {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    tokio::spawn(async move {
        for it in items {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            if tx.send(it).await.is_err() {
                return;
            }
        }
    });
    ReceiverStream(rx)
}

struct ReceiverStream<T>(tokio::sync::mpsc::Receiver<T>);

impl<T> futures_core::Stream for ReceiverStream<T> {
    type Item = T;
    fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<T>> {
        self.0.poll_recv(cx)
    }
}
