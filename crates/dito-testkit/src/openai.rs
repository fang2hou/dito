use axum::Json;
use axum::extract::{Multipart, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use dito_core::provider::mock;
use serde_json::{Value, json};

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
    let polished = mock::rule_polish(msg("user").as_deref().unwrap_or_default());
    let finish_reason = if s.behavior().truncate_chat { "length" } else { "stop" };
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
