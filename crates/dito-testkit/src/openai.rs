use std::io;
use std::time::Duration;

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Multipart, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use dito_core::provider::mock;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::recorded::{Recorded, header_pairs};
use crate::shared::SharedState;

pub(crate) async fn transcribe(State(s): SharedState, headers: HeaderMap, mut mp: Multipart) -> Response {
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
    s.record(rec);
    if let Some(r) = s.gate(&headers).await {
        return r;
    }
    let b = s.behavior();
    Json(json!({ "text": b.transcript, "language": b.language })).into_response()
}

pub(crate) async fn chat(State(s): SharedState, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    let msg = |role: &str| {
        body["messages"]
            .as_array()
            .and_then(|m| m.iter().rfind(|x| x["role"] == role))
            .and_then(|x| x["content"].as_str())
            .map(str::to_string)
    };
    s.record(Recorded {
        path: "chat/completions".into(),
        model: body["model"].as_str().map(str::to_string),
        system: msg("system"),
        user: msg("user"),
        reasoning_effort: body["reasoning_effort"].as_str().map(str::to_string),
        body: Some(body.clone()),
        headers: header_pairs(&headers),
        ..Default::default()
    });
    if let Some(r) = s.gate(&headers).await {
        return r;
    }
    if s.behavior().reject_reasoning_effort && body.get("reasoning_effort").is_some() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": {"message": "Unsupported parameter: 'reasoning_effort' is not supported with this model."}})),
        )
            .into_response();
    }
    let polished =
        mock::rule_reply(msg("system").as_deref().unwrap_or_default(), msg("user").as_deref().unwrap_or_default());
    let finish_reason = if s.behavior().truncate_chat { "length" } else { "stop" };
    if body["stream"] == true {
        return stream_reply(&polished, finish_reason, s.behavior().chat_chunk_delay);
    }
    Json(json!({
        "id": "chatcmpl-mock",
        "object": "chat.completion",
        "choices": [{
            "index": 0,
            "finish_reason": finish_reason,
            "message": { "role": "assistant", "content": polished }
        }]
    }))
    .into_response()
}

const PIECE_CHARS: usize = 4;

/// The reply as server-sent events: a role frame, the text in pieces of a few characters, a finish frame and `[DONE]`.
fn stream_reply(text: &str, finish_reason: &str, delay: Duration) -> Response {
    let frame = |delta: Value, finish: Value| {
        let event = json!({
            "id": "chatcmpl-mock",
            "object": "chat.completion.chunk",
            "choices": [{ "index": 0, "delta": delta, "finish_reason": finish }]
        });
        format!("data: {event}\n\n")
    };
    let chars: Vec<char> = text.chars().collect();
    let mut events = vec![frame(json!({ "role": "assistant", "content": "" }), Value::Null)];
    events.extend(
        chars
            .chunks(PIECE_CHARS)
            .map(|piece| frame(json!({ "content": piece.iter().collect::<String>() }), Value::Null)),
    );
    events.push(frame(json!({}), finish_reason.into()));
    events.push("data: [DONE]\n\n".into());

    let (tx, rx) = mpsc::channel(1);
    tokio::spawn(async move {
        for (i, event) in events.into_iter().enumerate() {
            if i > 0 && !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            if tx.send(Ok(Bytes::from(event))).await.is_err() {
                return;
            }
        }
    });
    let mut resp = Response::new(Body::from_stream(EventStream(rx)));
    resp.headers_mut().insert("content-type", HeaderValue::from_static("text/event-stream"));
    resp
}

struct EventStream(mpsc::Receiver<io::Result<Bytes>>);

impl futures_core::Stream for EventStream {
    type Item = io::Result<Bytes>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.0.poll_recv(cx)
    }
}
