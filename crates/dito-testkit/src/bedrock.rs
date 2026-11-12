use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use dito_core::provider::mock;
use serde_json::{Value, json};

use crate::recorded::{Recorded, header_pairs};
use crate::shared::SharedState;

pub(crate) async fn converse(
    State(s): SharedState,
    headers: HeaderMap,
    Path(model): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let content =
        body["messages"].as_array().and_then(|turns| turns.last()).and_then(|turn| turn["content"].as_array());
    let user = content.and_then(|c| c.iter().find_map(|x| x["text"].as_str())).map(str::to_string);
    s.record(Recorded {
        path: "converse".into(),
        model: Some(model),
        system: body["system"][0]["text"].as_str().map(str::to_string),
        user: user.clone(),
        body: Some(body.clone()),
        headers: header_pairs(&headers),
        ..Default::default()
    });
    let b = s.behavior();
    if !b.latency.is_zero() {
        tokio::time::sleep(b.latency).await;
    }
    if let Some(code) = b.fail_status {
        return service_error(code);
    }
    let text = mock::rule_polish(&user.unwrap_or_default());
    Json(json!({
        "output": { "message": { "role": "assistant", "content": [{ "text": text }] } },
        "stopReason": "end_turn",
        "usage": { "inputTokens": 1, "outputTokens": 1, "totalTokens": 2 },
        "metrics": { "latencyMs": 1 }
    }))
    .into_response()
}

fn service_error(code: u16) -> Response {
    let (kind, status) = match code {
        403 | 401 => ("AccessDeniedException", 403),
        429 => ("ThrottlingException", 429),
        404 => ("ResourceNotFoundException", 404),
        other => ("InternalServerException", other),
    };
    let mut headers = HeaderMap::new();
    headers.insert("x-amzn-errortype", HeaderValue::from_static(kind));
    let status = StatusCode::from_u16(status).expect("fail_status is a valid HTTP status");
    (status, headers, Json(json!({"message": "forced failure"}))).into_response()
}
