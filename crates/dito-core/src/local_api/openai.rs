//! The OpenAI-compatible surface under `/v1`: how a request maps to the chat contract and a reply maps back.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{ErrorKind, ProviderError};
use crate::provider::{ChatMessage, ChatRequest, ChatRole};

/// An error in the shape OpenAI clients parse: `{"error": {"message", "type", "code"}}`.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ApiError {
    pub(super) status: StatusCode,
    pub(super) kind: &'static str,
    pub(super) code: &'static str,
    pub(super) message: String,
}

impl ApiError {
    pub(super) fn invalid(code: &'static str, message: impl Into<String>) -> Self {
        Self { status: StatusCode::BAD_REQUEST, kind: "invalid_request_error", code, message: message.into() }
    }

    pub(super) fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            kind: "invalid_request_error",
            code: "invalid_api_key",
            message: "Send the access token as Authorization: Bearer <token>.".into(),
        }
    }

    pub(super) fn forbidden() -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            kind: "invalid_request_error",
            code: "forbidden",
            message: "Only requests to the loopback address, without a browser origin, are served.".into(),
        }
    }

    pub(super) fn body(&self) -> serde_json::Value {
        json!({ "error": { "message": self.message, "type": self.kind, "code": self.code } })
    }
}

/// What a client may learn about a failed engine call: the kind of failure. The detail can name the endpoint or the
/// account behind a cloud configuration, which the client must never see, so it goes to the log instead.
impl From<&ProviderError> for ApiError {
    fn from(e: &ProviderError) -> Self {
        let (status, code, message) = match e.kind {
            ErrorKind::Auth => {
                (StatusCode::BAD_GATEWAY, "upstream_auth", "The model provider rejected Dito's credentials.")
            }
            ErrorKind::Network => {
                (StatusCode::BAD_GATEWAY, "upstream_unreachable", "The model provider could not be reached.")
            }
            ErrorKind::Timeout => {
                (StatusCode::GATEWAY_TIMEOUT, "upstream_timeout", "The model took too long to answer.")
            }
            ErrorKind::RateLimit => {
                (StatusCode::TOO_MANY_REQUESTS, "rate_limit_exceeded", "The model provider is rate limiting Dito.")
            }
            ErrorKind::ModelNotFound => {
                (StatusCode::BAD_GATEWAY, "upstream_model_not_found", "The configured model does not exist.")
            }
            ErrorKind::BadResponse => {
                (StatusCode::BAD_GATEWAY, "upstream_bad_response", "The model returned a reply Dito could not read.")
            }
            ErrorKind::Unavailable => {
                (StatusCode::SERVICE_UNAVAILABLE, "model_unavailable", "The model is not installed or not running.")
            }
            ErrorKind::NoSpeech | ErrorKind::Mic | ErrorKind::Other => {
                (StatusCode::INTERNAL_SERVER_ERROR, "server_error", "The model could not answer.")
            }
        };
        tracing::warn!("local api: {e}");
        Self { status, kind: "api_error", code, message: message.into() }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (self.status, axum::Json(self.body())).into_response();
        if self.status == StatusCode::UNAUTHORIZED {
            response
                .headers_mut()
                .insert(axum::http::header::WWW_AUTHENTICATE, "Bearer".parse().expect("static value"));
        }
        response
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct ChatCompletionRequest {
    #[serde(default)]
    pub(super) messages: Vec<WireMessage>,
    #[serde(default)]
    pub(super) stream: bool,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_tokens: Option<u32>,
    max_completion_tokens: Option<u32>,
    n: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(super) struct WireMessage {
    role: String,
    content: Option<Content>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Parts(Vec<Part>),
}

#[derive(Debug, Deserialize)]
struct Part {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

impl Content {
    fn into_text(self) -> Result<String, ApiError> {
        match self {
            Self::Text(text) => Ok(text),
            Self::Parts(parts) => parts
                .into_iter()
                .map(|part| match (part.kind.as_str(), part.text) {
                    ("text", Some(text)) => Ok(text),
                    (kind, _) => Err(ApiError::invalid(
                        "unsupported_content",
                        format!("Content parts of type \"{kind}\" are not supported; only text."),
                    )),
                })
                .collect::<Result<Vec<_>, _>>()
                .map(|texts| texts.join("\n")),
        }
    }
}

impl ChatCompletionRequest {
    /// The model field is not read: Dito answers with the text model it is set to.
    pub(super) fn into_chat(self) -> Result<ChatRequest, ApiError> {
        if self.n.is_some_and(|n| n != 1) {
            return Err(ApiError::invalid("unsupported_n", "Only n = 1 is supported."));
        }
        let mut messages = Vec::with_capacity(self.messages.len());
        for message in self.messages {
            let role = match message.role.as_str() {
                "system" | "developer" => ChatRole::System,
                "user" => ChatRole::User,
                "assistant" => ChatRole::Assistant,
                other => {
                    return Err(ApiError::invalid(
                        "unsupported_role",
                        format!("Messages with the role \"{other}\" are not supported."),
                    ));
                }
            };
            let content = message.content.map(Content::into_text).transpose()?.unwrap_or_default();
            messages.push(ChatMessage::new(role, content));
        }
        if !messages.iter().any(|m| m.role != ChatRole::System) {
            return Err(ApiError::invalid("missing_messages", "messages needs at least one user message."));
        }
        Ok(ChatRequest {
            messages,
            temperature: self.temperature,
            top_p: self.top_p,
            max_tokens: self.max_completion_tokens.or(self.max_tokens),
            thinking: None,
        })
    }
}

pub(super) fn completion_id() -> String {
    format!("chatcmpl-{:x}", unix_nanos())
}

pub(super) fn unix_seconds() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn unix_nanos() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())
}

pub(super) fn completion(id: &str, created: u64, model: &str, reply: &str) -> serde_json::Value {
    json!({
        "id": id,
        "object": "chat.completion",
        "created": created,
        "model": model,
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": reply }, "finish_reason": "stop" }],
    })
}

/// One `chat.completion.chunk`: the opening role, a piece of the reply, or the end.
pub(super) fn chunk(id: &str, created: u64, model: &str, delta: Chunk<'_>) -> serde_json::Value {
    let (delta, finish) = match delta {
        Chunk::Role => (json!({ "role": "assistant", "content": "" }), serde_json::Value::Null),
        Chunk::Text(text) => (json!({ "content": text }), serde_json::Value::Null),
        Chunk::Stop => (json!({}), json!("stop")),
    };
    json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [{ "index": 0, "delta": delta, "finish_reason": finish }],
    })
}

pub(super) enum Chunk<'a> {
    Role,
    Text(&'a str),
    Stop,
}

#[derive(Serialize)]
pub(super) struct Transcription<'a> {
    pub(super) text: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(body: serde_json::Value) -> Result<ChatRequest, ApiError> {
        serde_json::from_value::<ChatCompletionRequest>(body).unwrap().into_chat()
    }

    #[test]
    fn messages_and_sampling_map_to_the_chat_contract() {
        let request = parse(json!({
            "model": "anything",
            "messages": [
                { "role": "developer", "content": "Be brief." },
                { "role": "user", "content": [{ "type": "text", "text": "a" }, { "type": "text", "text": "b" }] },
                { "role": "assistant", "content": "ok" },
                { "role": "user", "content": "go" },
            ],
            "temperature": 0.3,
            "top_p": 0.9,
            "max_tokens": 50,
            "max_completion_tokens": 80,
        }))
        .unwrap();
        let roles: Vec<_> = request.messages.iter().map(|m| (m.role, m.content.as_str())).collect();
        assert_eq!(
            roles,
            [
                (ChatRole::System, "Be brief."),
                (ChatRole::User, "a\nb"),
                (ChatRole::Assistant, "ok"),
                (ChatRole::User, "go"),
            ]
        );
        assert_eq!((request.temperature, request.top_p, request.max_tokens), (Some(0.3), Some(0.9), Some(80)));
    }

    #[test]
    fn what_the_chat_contract_cannot_carry_is_refused_with_a_code() {
        let code = |body| parse(body).unwrap_err().code;
        assert_eq!(code(json!({ "messages": [] })), "missing_messages");
        assert_eq!(code(json!({ "messages": [{ "role": "system", "content": "x" }] })), "missing_messages");
        assert_eq!(code(json!({ "messages": [{ "role": "tool", "content": "x" }] })), "unsupported_role");
        assert_eq!(code(json!({ "n": 2, "messages": [{ "role": "user", "content": "x" }] })), "unsupported_n");
        let image = json!({ "messages": [{ "role": "user", "content": [{ "type": "image_url" }] }] });
        assert_eq!(code(image), "unsupported_content");
    }

    #[test]
    fn replies_take_the_completion_and_chunk_shapes() {
        let done = completion("chatcmpl-1", 7, "m", "hi");
        assert_eq!(done["choices"][0]["message"]["content"], "hi");
        assert_eq!(done["choices"][0]["finish_reason"], "stop");
        assert_eq!(chunk("c", 7, "m", Chunk::Role)["choices"][0]["delta"]["role"], "assistant");
        assert_eq!(chunk("c", 7, "m", Chunk::Text("x"))["choices"][0]["delta"]["content"], "x");
        let stop = chunk("c", 7, "m", Chunk::Stop);
        assert_eq!(
            (stop["choices"][0]["finish_reason"].as_str(), stop["object"].as_str()),
            (Some("stop"), Some("chat.completion.chunk"))
        );
    }

    #[test]
    fn a_provider_failure_tells_the_client_its_kind_and_nothing_about_the_endpoint() {
        let secret = ProviderError::new(ErrorKind::Auth, "HTTP 401 from https://internal.example/v1 for key sk-123");
        let api = ApiError::from(&secret);
        assert_eq!((api.status, api.code), (StatusCode::BAD_GATEWAY, "upstream_auth"));
        let body = api.body().to_string();
        assert!(!body.contains("internal.example") && !body.contains("sk-123"), "{body}");
        assert_eq!(
            ApiError::from(&ProviderError::new(ErrorKind::RateLimit, "x")).status,
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            ApiError::from(&ProviderError::new(ErrorKind::Unavailable, "x")).status,
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
}
