//! Azure OpenAI and OpenAI-compatible chat engines for polishing (one wire format, two URL/auth schemes).

use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use serde_json::{Number, Value, json};

use super::{
    ChatCancel, ChatDelta, ChatRequest, ChatRole, TextModel, TextRequest, Thinking, default_timeout, next_stream_item,
    preconnect, redirects_except_for_preconnect,
};
use crate::config::CloudTextParams;
use crate::error::{ErrorKind, ProviderError, ProviderResult};

#[derive(Debug, Clone)]
pub enum Flavor {
    /// `{endpoint}/openai/v1/...`, `api-key` header, `model` = deployment name.
    Azure { endpoint: String, deployment: String },
    /// `{base_url}/...`, optional `Authorization: Bearer`.
    Compat { base_url: String, model: String },
}

/// A streamed reply may run far longer than a one-shot polish; the silence between two chunks is bounded instead.
const STREAM_TOTAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

#[derive(Clone)]
pub struct OpenAiEngine {
    flavor: Flavor,
    api_key: String,
    params: CloudTextParams,
    client: reqwest::Client,
    /// `reasoning_effort` values the server refused, left out of later requests so only the first one pays for the
    /// refusal. Kept per value: a model can refuse one level and take another.
    refused_efforts: Arc<Mutex<Vec<Value>>>,
}

fn trim_known_suffix(url: &str) -> &str {
    let u = url.trim().trim_end_matches('/');
    u.strip_suffix("/chat/completions").unwrap_or(u)
}

/// A parameter as the number the user typed: the f32 `0.2` is `0.2` on the wire, not the nearest f64
/// `0.20000000298023224`. A value JSON cannot hold (NaN, infinity) is not sent.
fn json_number(value: f32) -> Option<Value> {
    value.to_string().parse::<f64>().ok().and_then(Number::from_f64).map(Value::Number)
}

fn take_effort(body: &mut Value) -> Option<Value> {
    body.as_object_mut().expect("chat body is a JSON object").remove("reasoning_effort")
}

impl OpenAiEngine {
    pub fn new(flavor: Flavor, api_key: impl Into<String>, params: CloudTextParams) -> ProviderResult<Self> {
        let (name, value) = match &flavor {
            Flavor::Azure { endpoint, deployment } => ("endpoint / deployment", (endpoint.trim(), deployment.trim())),
            Flavor::Compat { base_url, model } => ("base URL / model", (base_url.trim(), model.trim())),
        };
        if value.0.is_empty() || value.1.is_empty() {
            return Err(ProviderError::new(ErrorKind::Unavailable, format!("{name} is not set")));
        }
        let client = reqwest::Client::builder()
            .retry(super::dropped_connection::resend_once(value.0))
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(default_timeout())
            .redirect(redirects_except_for_preconnect())
            .build()
            .map_err(|e| ProviderError::new(ErrorKind::Other, e.to_string()))?;
        Ok(Self { flavor, api_key: api_key.into(), params, client, refused_efforts: Arc::default() })
    }

    fn url(&self, path: &str) -> String {
        match &self.flavor {
            Flavor::Azure { endpoint, .. } => format!("{}/openai/v1/{path}", endpoint.trim().trim_end_matches('/')),
            Flavor::Compat { base_url, .. } => format!("{}/{path}", trim_known_suffix(base_url)),
        }
    }

    fn model(&self) -> &str {
        match &self.flavor {
            Flavor::Azure { deployment, .. } => deployment.trim(),
            Flavor::Compat { model, .. } => model.trim(),
        }
    }

    /// Azure's current surface takes `max_completion_tokens` (reasoning models refuse `max_tokens`); the compatible
    /// servers know `max_tokens`.
    fn token_limit_field(&self) -> &'static str {
        match self.flavor {
            Flavor::Azure { .. } => "max_completion_tokens",
            Flavor::Compat { .. } => "max_tokens",
        }
    }

    fn body(&self, req: &TextRequest<'_>) -> Value {
        let mut messages = vec![json!({ "role": "system", "content": req.system })];
        for example in req.examples {
            messages.push(json!({ "role": "user", "content": example.said }));
            messages.push(json!({ "role": "assistant", "content": example.written }));
        }
        messages.push(json!({ "role": "user", "content": req.text }));
        self.finish_body(messages, None)
    }

    fn chat_body(&self, req: &ChatRequest) -> Value {
        let messages = req
            .messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    ChatRole::System => "system",
                    ChatRole::User => "user",
                    ChatRole::Assistant => "assistant",
                };
                json!({ "role": role, "content": m.content })
            })
            .collect();
        let mut body = self.finish_body(messages, Some(req));
        body["stream"] = true.into();
        body
    }

    /// A chat request's sampling fields replace the configured ones; `extra` still wins over both.
    fn finish_body(&self, messages: Vec<Value>, chat: Option<&ChatRequest>) -> Value {
        let mut body = json!({ "model": self.model(), "messages": messages });
        let params = &self.params;
        let temperature = chat.and_then(|c| c.temperature).or(params.temperature);
        let top_p = chat.and_then(|c| c.top_p).or(params.top_p);
        let max_tokens = chat.and_then(|c| c.max_tokens).or(params.max_tokens);
        if let Some(temperature) = temperature.and_then(json_number) {
            body["temperature"] = temperature;
        }
        if let Some(top_p) = top_p.and_then(json_number) {
            body["top_p"] = top_p;
        }
        if let Some(limit) = max_tokens {
            body[self.token_limit_field()] = limit.into();
        }
        if !params.reasoning_effort.as_str().trim().is_empty() {
            body["reasoning_effort"] = params.reasoning_effort.as_str().into();
        }
        if let Some(Thinking::Level(level)) = chat.and_then(|c| c.thinking.as_ref()) {
            body["reasoning_effort"] = level.as_str().into();
        }
        for (key, value) in &params.extra {
            body[key.as_str()] = value.clone();
        }
        if body.get("reasoning_effort").is_some_and(|effort| self.refused_efforts().contains(effort)) {
            take_effort(&mut body);
        }
        body
    }

    fn refused_efforts(&self) -> MutexGuard<'_, Vec<Value>> {
        self.refused_efforts.lock().expect("refused efforts mutex poisoned")
    }

    /// Called once the request has gone through without the value, which tells a refusal from an unrelated failure.
    fn remember_refused(&self, effort: Option<Value>) {
        let mut refused = self.refused_efforts();
        if let Some(effort) = effort
            && !refused.contains(&effort)
        {
            refused.push(effort);
        }
    }

    fn auth(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.api_key.is_empty() {
            return rb;
        }
        match self.flavor {
            Flavor::Azure { .. } => rb.header("api-key", &self.api_key),
            Flavor::Compat { .. } => rb.bearer_auth(&self.api_key),
        }
    }

    async fn send_json(&self, rb: reqwest::RequestBuilder) -> ProviderResult<Value> {
        let resp = rb.send().await.map_err(|e| ProviderError::from_reqwest(&e))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ProviderError::from_reqwest(&e))?;
        if !status.is_success() {
            return Err(ProviderError::from_status(status.as_u16(), &body));
        }
        serde_json::from_str(&body)
            .map_err(|e| ProviderError::new(ErrorKind::BadResponse, format!("invalid JSON: {e}")))
    }

    async fn completions(&self, body: &Value) -> ProviderResult<Value> {
        self.send_json(self.auth(self.client.post(self.url("chat/completions"))).json(body)).await
    }

    async fn open_stream(&self, body: &Value) -> ProviderResult<reqwest::Response> {
        let resp = self
            .auth(self.client.post(self.url("chat/completions")))
            .timeout(STREAM_TOTAL_TIMEOUT)
            .json(body)
            .send()
            .await
            .map_err(|e| ProviderError::from_reqwest(&e))?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let text = resp.text().await.map_err(|e| ProviderError::from_reqwest(&e))?;
        Err(ProviderError::from_status(status.as_u16(), &text))
    }
}

/// What one `data:` payload of a streamed chat completion says.
#[derive(Debug, PartialEq, Eq)]
enum Frame {
    Text(String),
    Done,
    Skip,
}

fn parse_frame(data: &str) -> ProviderResult<Frame> {
    let data = data.trim();
    if data == "[DONE]" {
        return Ok(Frame::Done);
    }
    let v: Value = serde_json::from_str(data)
        .map_err(|e| ProviderError::new(ErrorKind::BadResponse, format!("invalid stream event: {e}")))?;
    if let Some(error) = v.get("error").filter(|e| !e.is_null()) {
        let message = error.get("message").and_then(Value::as_str).unwrap_or("stream error");
        return Err(ProviderError::new(ErrorKind::Other, message));
    }
    // Azure opens with a frame that has no choices; role and finish frames carry no content.
    Ok(match v.pointer("/choices/0/delta/content").and_then(Value::as_str) {
        Some(text) if !text.is_empty() => Frame::Text(text.to_string()),
        _ => Frame::Skip,
    })
}

/// Splits a server-sent event stream into `data:` payloads. Bytes are decoded only once their line is complete, so a
/// multi-byte character cut by a chunk boundary survives.
#[derive(Default)]
struct SseLines {
    pending: Vec<u8>,
}

impl SseLines {
    fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(chunk);
        let mut payloads = Vec::new();
        while let Some(end) = self.pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            if let Some(data) = line.trim_end_matches(['\r', '\n']).strip_prefix("data:") {
                payloads.push(data.strip_prefix(' ').unwrap_or(data).to_string());
            }
        }
        payloads
    }
}

fn polish_response(v: Value) -> ProviderResult<String> {
    if v.pointer("/choices/0/finish_reason").and_then(Value::as_str) == Some("length") {
        return Err(ProviderError::retryable(ErrorKind::BadResponse, "output was cut off at the token limit"));
    }
    v.pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .ok_or_else(|| ProviderError::retryable(ErrorKind::BadResponse, "no content in response"))
}

#[async_trait]
impl TextModel for OpenAiEngine {
    async fn polish(&self, req: &TextRequest<'_>) -> ProviderResult<String> {
        let mut body = self.body(req);
        let v = match self.completions(&body).await {
            // Non-reasoning models reject the field; retry once without it.
            Err(e) if e.kind == ErrorKind::Other && e.message.contains("reasoning_effort") => {
                let effort = take_effort(&mut body);
                let v = self.completions(&body).await?;
                self.remember_refused(effort);
                v
            }
            other => other?,
        };
        polish_response(v)
    }

    async fn preconnect(&self) {
        preconnect(&self.client, &self.url("chat/completions")).await;
    }

    /// The reply is the exact concatenation of the deltas. A reply that ends at the token limit is returned as it
    /// stands, since its text is already on screen; one with no text at all is an error.
    async fn chat(&self, req: &ChatRequest, on_delta: ChatDelta<'_>, cancel: &ChatCancel) -> ProviderResult<String> {
        if cancel.is_cancelled() {
            return Ok(String::new());
        }
        let mut body = self.chat_body(req);
        let mut resp = match self.open_stream(&body).await {
            Err(e) if e.kind == ErrorKind::Other && e.message.contains("reasoning_effort") => {
                let effort = take_effort(&mut body);
                let resp = self.open_stream(&body).await?;
                self.remember_refused(effort);
                resp
            }
            other => other?,
        };
        let (mut lines, mut reply) = (SseLines::default(), String::new());
        'stream: while !cancel.is_cancelled() {
            let next = async {
                resp.chunk().await.map(|bytes| bytes.map(|b| b.to_vec())).map_err(|e| ProviderError::from_reqwest(&e))
            };
            let Some(chunk) = next_stream_item(next, cancel).await? else { break };
            for data in lines.push(&chunk) {
                match parse_frame(&data)? {
                    Frame::Text(piece) => {
                        on_delta(&piece);
                        reply.push_str(&piece);
                    }
                    Frame::Done => break 'stream,
                    Frame::Skip => {}
                }
                if cancel.is_cancelled() {
                    break 'stream;
                }
            }
        }
        if reply.is_empty() && !cancel.is_cancelled() {
            return Err(ProviderError::new(ErrorKind::BadResponse, "no content in response"));
        }
        Ok(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compat(base: &str) -> OpenAiEngine {
        OpenAiEngine::new(Flavor::Compat { base_url: base.into(), model: "m".into() }, "", CloudTextParams::default())
            .unwrap()
    }

    #[test]
    fn only_incomplete_polish_responses_are_retryable() {
        let length = json!({"choices": [{"finish_reason": "length", "message": {"content": "partial"}}]});
        let empty = json!({"choices": [{"finish_reason": "stop", "message": {"content": "  "}}]});
        let complete = json!({"choices": [{"finish_reason": "stop", "message": {"content": "done"}}]});
        assert!(polish_response(length).unwrap_err().is_retryable());
        assert!(polish_response(empty).unwrap_err().is_retryable());
        assert_eq!(polish_response(complete), Ok("done".into()));
        assert!(!ProviderError::from_status(429, "slow").is_retryable());
        assert!(!ProviderError::from_status(401, "no").is_retryable());
        assert!(!ProviderError::from_status(404, "gone").is_retryable());
        assert!(!ProviderError::from_status(408, "late").is_retryable());
        assert!(!ProviderError::from_status(504, "late").is_retryable());
    }

    #[test]
    fn compat_urls_tolerate_pasted_full_paths() {
        for base in ["http://h/v1", "http://h/v1/", "http://h/v1/chat/completions", "http://h/v1/chat/completions/"] {
            assert_eq!(compat(base).url("chat/completions"), "http://h/v1/chat/completions", "{base}");
        }
    }

    #[test]
    fn azure_urls_use_the_v1_surface() {
        let e = OpenAiEngine::new(
            Flavor::Azure { endpoint: "https://x.openai.azure.com/".into(), deployment: "gpt-6.1-terra".into() },
            "k",
            CloudTextParams::default(),
        )
        .unwrap();
        assert_eq!(e.url("chat/completions"), "https://x.openai.azure.com/openai/v1/chat/completions");
        assert_eq!(e.model(), "gpt-6.1-terra");
    }

    #[test]
    fn parameters_go_out_as_the_numbers_they_were_typed_as() {
        assert_eq!(json_number(0.2), Some(json!(0.2)));
        assert_eq!(json_number(0.95), Some(json!(0.95)));
        assert_eq!(json_number(1.0), Some(json!(1.0)));
        assert_eq!(json_number(f32::NAN), None);
        assert_eq!(json_number(f32::INFINITY), None);
    }

    #[test]
    fn extra_wins_over_every_other_field_of_the_body() {
        let params = CloudTextParams {
            temperature: Some(0.2),
            extra: serde_json::from_str(r#"{"temperature": 0.9, "seed": 42, "model": "other"}"#).unwrap(),
            ..CloudTextParams::default()
        };
        let engine =
            OpenAiEngine::new(Flavor::Compat { base_url: "http://h/v1".into(), model: "m".into() }, "", params)
                .unwrap();
        let body = engine.body(&TextRequest { system: "s", examples: &[], text: "t" });
        assert_eq!((&body["temperature"], &body["seed"], &body["model"]), (&json!(0.9), &json!(42), &json!("other")));
    }

    #[test]
    fn sse_lines_survive_chunk_boundaries_inside_a_character_and_crlf() {
        let event = "data: {\"choices\":[{\"delta\":{\"content\":\"明日\"}}]}\r\n\r\n".as_bytes();
        let cut = event.iter().position(|&b| b == 0xE6).unwrap() + 1;
        let mut sse = SseLines::default();
        assert!(sse.push(&event[..cut]).is_empty());
        let payloads = sse.push(&event[cut..]);
        assert_eq!(payloads.len(), 1);
        assert_eq!(parse_frame(&payloads[0]).unwrap(), Frame::Text("明日".into()));
        assert_eq!(sse.push(b": keep-alive\ndata:[DONE]\n"), ["[DONE]"]);
    }

    #[test]
    fn frames_without_text_are_skipped_and_stream_errors_surface() {
        for data in [
            r#"{"choices":[],"prompt_filter_results":[]}"#,
            r#"{"choices":[{"delta":{"role":"assistant","content":null}}]}"#,
            r#"{"choices":[{"delta":{"reasoning_content":"hmm"}}]}"#,
            r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
        ] {
            assert_eq!(parse_frame(data).unwrap(), Frame::Skip, "{data}");
        }
        assert_eq!(parse_frame("[DONE]").unwrap(), Frame::Done);
        let error = parse_frame(r#"{"error":{"message":"overloaded"}}"#).unwrap_err();
        assert_eq!((error.kind, error.message.as_str()), (ErrorKind::Other, "overloaded"));
        assert_eq!(parse_frame("not json").unwrap_err().kind, ErrorKind::BadResponse);
    }
}
