//! Azure OpenAI and OpenAI-compatible chat engines for polishing (one wire format, two URL/auth schemes).

use std::borrow::Cow;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use futures_util::{Stream, StreamExt, stream};
use serde_json::{Number, Value, json};

use super::{
    ChatCancel, ChatDelta, ChatMessage, ChatRequest, ChatRole, TextModel, TextRequest, Thinking, default_timeout,
    next_stream_item, preconnect, redirects_except_for_preconnect,
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
        let mut body = self.finish_body(merged_messages(&req.messages), Some(req));
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

/// Consecutive turns of one role share a message, since servers that want the roles to alternate refuse them. An
/// empty turn adds nothing to the message it joins.
fn merged_messages(messages: &[ChatMessage]) -> Vec<Value> {
    let mut turns: Vec<(ChatRole, Vec<&str>)> = Vec::new();
    for m in messages {
        match turns.last_mut() {
            Some((role, texts)) if *role == m.role => texts.push(&m.content),
            _ => turns.push((m.role, vec![&m.content])),
        }
    }
    turns
        .into_iter()
        .map(|(role, texts)| {
            let role = match role {
                ChatRole::System => "system",
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
            };
            let texts = texts.iter().filter(|text| !text.is_empty()).copied().collect::<Vec<_>>();
            json!({ "role": role, "content": texts.join("\n\n") })
        })
        .collect()
}

/// What one `data:` payload of a streamed chat completion says.
#[derive(Debug, PartialEq, Eq)]
enum Frame {
    /// A piece of the reply; `cut_off` when the frame's `finish_reason` is `length`, the token limit.
    Delta {
        text: String,
        cut_off: bool,
    },
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
    let text = v.pointer("/choices/0/delta/content").and_then(Value::as_str).unwrap_or_default();
    let cut_off = v.pointer("/choices/0/finish_reason").and_then(Value::as_str) == Some("length");
    // Azure opens with a frame that has no choices; role and finish frames carry no content.
    if text.is_empty() && !cut_off {
        return Ok(Frame::Skip);
    }
    Ok(Frame::Delta { text: text.to_string(), cut_off })
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

const THINK_OPEN: &str = "<think>";
const THINK_CLOSE: &str = "</think>";

/// The content after the leading `<think>...</think>` blocks, the reasoning that servers of reasoning models put
/// into the content. `None` while one of them is still open: what they hold is never the text.
fn after_leading_think(content: &str) -> Option<&str> {
    let mut rest = content;
    while let Some(reasoning) = rest.trim_start().strip_prefix(THINK_OPEN) {
        rest = reasoning.split_once(THINK_CLOSE)?.1;
    }
    Some(rest)
}

/// Retrying would spend the same budget on the same cutoff.
fn polish_response(v: Value) -> ProviderResult<String> {
    if v.pointer("/choices/0/finish_reason").and_then(Value::as_str) == Some("length") {
        return Err(ProviderError::new(ErrorKind::BadResponse, "output was cut off at the token limit"));
    }
    let content = v.pointer("/choices/0/message/content").and_then(Value::as_str).unwrap_or_default();
    let Some(text) = after_leading_think(content) else {
        return Err(ProviderError::retryable(ErrorKind::BadResponse, "the <think> block is never closed"));
    };
    if text.trim().is_empty() {
        return Err(ProviderError::retryable(ErrorKind::BadResponse, "no content in response"));
    }
    Ok(text.trim().to_string())
}

/// How far a reply has got past a leading `<think>` block.
#[derive(Debug, Default, PartialEq, Eq)]
enum ThinkStage {
    /// Nothing has shown yet whether the reply opens with the block.
    #[default]
    Opening,
    /// Inside the block, which is never the reply.
    Reasoning,
    /// Right after `</think>`: whitespace before the reply is dropped.
    After,
    /// The reply itself.
    Reply,
}

/// Drops the leading `<think>...</think>` blocks from a reply that arrives in pieces, with their tags split
/// across them. A piece is held back only while it may still start a tag, so a reply without the blocks streams
/// as it arrives.
#[derive(Debug, Default)]
struct ThinkFilter {
    stage: ThinkStage,
    held: String,
}

impl ThinkFilter {
    /// The part of `piece` that belongs to the reply.
    fn push<'a>(&mut self, piece: &'a str) -> Cow<'a, str> {
        if self.stage == ThinkStage::Reply {
            return Cow::Borrowed(piece);
        }
        self.held.push_str(piece);
        let mut out = String::new();
        loop {
            match self.stage {
                ThinkStage::Opening => {
                    let text = self.held.trim_start();
                    if text.starts_with(THINK_OPEN) {
                        let after = self.held.len() - text.len() + THINK_OPEN.len();
                        self.held.drain(..after);
                        self.stage = ThinkStage::Reasoning;
                    } else if text.is_empty() || THINK_OPEN.starts_with(text) {
                        break;
                    } else {
                        out.push_str(&self.held);
                        self.held.clear();
                        self.stage = ThinkStage::Reply;
                    }
                }
                ThinkStage::Reasoning => match self.held.find(THINK_CLOSE) {
                    Some(at) => {
                        self.held.drain(..at + THINK_CLOSE.len());
                        self.stage = ThinkStage::After;
                    }
                    None => {
                        self.held.drain(..self.held.len() - partial_tag(&self.held, THINK_CLOSE));
                        break;
                    }
                },
                ThinkStage::After => {
                    let leading = self.held.len() - self.held.trim_start().len();
                    self.held.drain(..leading);
                    self.stage = ThinkStage::Opening;
                }
                ThinkStage::Reply => break,
            }
        }
        Cow::Owned(out)
    }

    /// What a held piece turned out to be once the stream is over: the start of a reply that does not open with
    /// the block, never reasoning.
    fn finish(&mut self) -> &str {
        if self.stage != ThinkStage::Opening {
            self.held.clear();
        }
        self.stage = ThinkStage::Reply;
        &self.held
    }
}

/// The length of the longest start of `tag` that ends `text`, which the next piece may complete.
fn partial_tag(text: &str, tag: &str) -> usize {
    (1..tag.len()).rev().find(|&len| text.ends_with(&tag[..len])).unwrap_or(0)
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

    /// The reply is the concatenation of the deltas, without a leading `<think>` block. A reply that ends at the
    /// token limit is returned as it stands, since its text is already on screen; one with no text at all is an error.
    async fn chat(&self, req: &ChatRequest, on_delta: ChatDelta<'_>, cancel: &ChatCancel) -> ProviderResult<String> {
        if cancel.is_cancelled() {
            return Ok(String::new());
        }
        let mut body = self.chat_body(req);
        let resp = match self.open_stream(&body).await {
            Err(e) if e.kind == ErrorKind::Other && e.message.contains("reasoning_effort") => {
                let effort = take_effort(&mut body);
                let resp = self.open_stream(&body).await?;
                self.remember_refused(effort);
                resp
            }
            other => other?,
        };
        read_stream(chunks(resp), on_delta, cancel).await
    }
}

/// The body of a streamed chat completion as its chunks arrive; a chunk of `None` ends it.
fn chunks(resp: reqwest::Response) -> impl Stream<Item = ProviderResult<Option<Vec<u8>>>> {
    stream::unfold(resp, |mut resp| async move {
        let next =
            resp.chunk().await.map(|bytes| bytes.map(|b| b.to_vec())).map_err(|e| ProviderError::from_reqwest(&e));
        Some((next, resp))
    })
}

/// Reads a streamed chat completion into its reply.
async fn read_stream(
    chunks: impl Stream<Item = ProviderResult<Option<Vec<u8>>>>,
    on_delta: ChatDelta<'_>,
    cancel: &ChatCancel,
) -> ProviderResult<String> {
    let mut chunks = std::pin::pin!(chunks);
    let (mut lines, mut think, mut reply) = (SseLines::default(), ThinkFilter::default(), String::new());
    let mut cut_off = false;
    let mut emit = |text: &str| {
        if !text.is_empty() {
            on_delta(text);
            reply.push_str(text);
        }
    };
    'stream: while !cancel.is_cancelled() {
        let next = async { chunks.next().await.unwrap_or(Ok(None)) };
        let Some(chunk) = next_stream_item(next, cancel).await? else { break };
        for data in lines.push(&chunk) {
            match parse_frame(&data)? {
                Frame::Delta { text, cut_off: at_limit } => {
                    cut_off |= at_limit;
                    emit(&think.push(&text));
                }
                Frame::Done => break 'stream,
                Frame::Skip => {}
            }
            if cancel.is_cancelled() {
                break 'stream;
            }
        }
    }
    // A stopped reply is what was shown: what was still held never was.
    if cancel.is_cancelled() {
        return Ok(reply);
    }
    emit(think.finish());
    if reply.trim().is_empty() {
        return Err(if cut_off {
            ProviderError::new(ErrorKind::BadResponse, "output was cut off at the token limit")
        } else {
            ProviderError::new(ErrorKind::BadResponse, "no content in response")
        });
    }
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compat(base: &str) -> OpenAiEngine {
        OpenAiEngine::new(Flavor::Compat { base_url: base.into(), model: "m".into() }, "", CloudTextParams::default())
            .unwrap()
    }

    fn response(finish_reason: &str, content: &str) -> Value {
        json!({"choices": [{"finish_reason": finish_reason, "message": {"content": content}}]})
    }

    #[test]
    fn only_an_empty_polish_response_is_retryable() {
        let cut_off = polish_response(response("length", "partial")).unwrap_err();
        assert_eq!((cut_off.kind, cut_off.is_retryable()), (ErrorKind::BadResponse, false));
        assert_eq!(cut_off.message, "output was cut off at the token limit");
        assert!(polish_response(response("stop", "  ")).unwrap_err().is_retryable());
        assert_eq!(polish_response(response("stop", "done")), Ok("done".into()));
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
        assert_eq!(parse_frame(&payloads[0]).unwrap(), Frame::Delta { text: "明日".into(), cut_off: false });
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

    #[test]
    fn a_leading_think_block_is_dropped_from_a_polish_response() {
        let polish = |content: &str| polish_response(response("stop", content));
        assert_eq!(polish("<think>reasoning</think>clean text"), Ok("clean text".into()));
        assert_eq!(polish(" <think>\nfirst\nsecond\n</think>\nclean"), Ok("clean".into()));
        assert_eq!(polish("<think>reasoning</think>").unwrap_err().message, "no content in response");
        assert_eq!(polish("<think>a</think>\n<think>b</think>\nhello"), Ok("hello".into()));
        let unclosed = polish("<think>a</think>\n<think>the block never closes").unwrap_err();
        assert!(unclosed.is_retryable());
        assert_eq!(unclosed.message, "the <think> block is never closed");
    }

    #[test]
    fn a_frame_that_ends_the_reply_at_the_token_limit_is_not_skipped() {
        let delta = |text: &str, cut_off: bool| Frame::Delta { text: text.into(), cut_off };
        let length = r#"{"choices":[{"delta":{},"finish_reason":"length"}]}"#;
        assert_eq!(parse_frame(length).unwrap(), delta("", true));
        let last_word = r#"{"choices":[{"delta":{"content":"last"},"finish_reason":"length"}]}"#;
        assert_eq!(parse_frame(last_word).unwrap(), delta("last", true));
    }

    #[test]
    fn consecutive_turns_of_one_role_share_a_message() {
        let message = |role, content: &str| ChatMessage::new(role, content);
        let messages = vec![
            message(ChatRole::System, "s"),
            message(ChatRole::User, "a"),
            message(ChatRole::User, "b"),
            message(ChatRole::Assistant, "c"),
            message(ChatRole::Assistant, "d"),
            message(ChatRole::User, "e"),
        ];
        let body = compat("http://h/v1").chat_body(&ChatRequest { messages, ..ChatRequest::default() });
        assert_eq!(
            body["messages"],
            json!([
                {"role": "system", "content": "s"},
                {"role": "user", "content": "a\n\nb"},
                {"role": "assistant", "content": "c\n\nd"},
                {"role": "user", "content": "e"},
            ])
        );
        // A system message never joins a turn, only another system message.
        let messages = vec![
            message(ChatRole::User, "a"),
            message(ChatRole::System, "s1"),
            message(ChatRole::System, "s2"),
            message(ChatRole::Assistant, "c"),
        ];
        assert_eq!(
            merged_messages(&messages),
            [
                json!({"role": "user", "content": "a"}),
                json!({"role": "system", "content": "s1\n\ns2"}),
                json!({"role": "assistant", "content": "c"}),
            ]
        );
        // An empty turn adds nothing to the message it joins.
        let messages = vec![
            message(ChatRole::User, ""),
            message(ChatRole::User, "hi"),
            message(ChatRole::Assistant, ""),
            message(ChatRole::Assistant, ""),
        ];
        assert_eq!(
            merged_messages(&messages),
            [json!({"role": "user", "content": "hi"}), json!({"role": "assistant", "content": ""})]
        );
    }

    /// One `data:` event of a streamed reply.
    fn frame(delta: Value, finish: Value) -> String {
        let event = json!({"choices": [{"index": 0, "delta": delta, "finish_reason": finish}]});
        format!("data: {event}\n\n")
    }

    /// The events of a reply that arrives as `pieces` and ends with `finish_reason`.
    fn sse(pieces: &[&str], finish_reason: &str) -> Vec<u8> {
        let mut events = frame(json!({"role": "assistant", "content": ""}), Value::Null);
        for piece in pieces {
            events.push_str(&frame(json!({"content": piece}), Value::Null));
        }
        events.push_str(&frame(json!({}), finish_reason.into()));
        events.push_str("data: [DONE]\n\n");
        events.into_bytes()
    }

    /// Reads the events in chunks of `size` bytes: the deltas shown, then the reply returned.
    async fn read(events: &[u8], size: usize) -> (Vec<String>, ProviderResult<String>) {
        let mut deltas = Vec::new();
        let chunks = stream::iter(events.chunks(size)).map(|chunk| Ok(Some(chunk.to_vec())));
        let reply = read_stream(chunks, &mut |piece| deltas.push(piece.to_string()), &ChatCancel::default()).await;
        (deltas, reply)
    }

    #[test]
    fn a_reply_without_a_think_block_streams_as_it_arrives() {
        let mut think = ThinkFilter::default();
        assert_eq!(think.push("hello "), "hello ");
        assert_eq!(think.push("there"), "there");
        // Only what may still be the start of the tag waits for the next piece.
        let mut think = ThinkFilter::default();
        assert_eq!(think.push("\n\n<"), "");
        assert_eq!(think.push("b>bold</b>"), "\n\n<b>bold</b>");
        let mut think = ThinkFilter::default();
        assert_eq!(think.push("<thi"), "");
        assert_eq!(think.finish(), "<thi");
    }

    #[tokio::test]
    async fn a_leading_think_block_is_dropped_from_a_streamed_reply_however_the_pieces_are_cut() {
        let events = sse(&[" \n<th", "ink>\nlet me think\n</th", "ink>\n\nhello ", "there"], "stop");
        for size in 1..=events.len() {
            let (deltas, reply) = read(&events, size).await;
            assert_eq!(reply, Ok("hello there".into()), "chunks of {size} bytes");
            assert_eq!(deltas.concat(), "hello there", "chunks of {size} bytes");
        }
        let events = sse(&["<think>考", "える</think>\n\n答", "え"], "stop");
        for size in 1..=events.len() {
            assert_eq!(read(&events, size).await.1, Ok("答え".into()), "chunks of {size} bytes");
        }
        // A second block goes the way of the first, and text that only starts like the tag streams at once.
        for (pieces, reply) in [
            (&["<think>a</think>\n<th", "ink>b</think>\n", "hello"][..], "hello"),
            (&["<think>a</think>", "<b>bold</b>"][..], "<b>bold</b>"),
            (&["<think>a</think>\n", "<think>b</think>\n\nhello"][..], "hello"),
        ] {
            let events = sse(pieces, "stop");
            for size in 1..=events.len() {
                assert_eq!(read(&events, size).await.1, Ok(reply.into()), "{reply} in chunks of {size} bytes");
            }
        }
    }

    #[tokio::test]
    async fn a_streamed_reply_of_reasoning_alone_is_an_error_that_names_the_token_limit() {
        let events = sse(&["<think>all ", "reasoning</think>"], "length");
        let (deltas, reply) = read(&events, 5).await;
        assert!(deltas.is_empty());
        let error = reply.unwrap_err();
        assert_eq!((error.kind, error.is_retryable()), (ErrorKind::BadResponse, false));
        assert_eq!(error.message, "output was cut off at the token limit");

        let events = sse(&["<think>all ", "reasoning</think>"], "stop");
        assert_eq!(read(&events, 5).await.1.unwrap_err().message, "no content in response");
        // Text already on screen is kept when the limit ends the reply.
        assert_eq!(read(&sse(&["hel", "lo the"], "length"), 5).await.1, Ok("hello the".into()));
    }

    #[tokio::test]
    async fn a_stopped_stream_returns_what_was_shown_and_shows_nothing_more() {
        let events = sse(&["hello ", "there"], "stop");
        let chunks = stream::iter([events]).map(|event| Ok(Some(event)));
        let cancel = ChatCancel::default();
        let mut deltas = Vec::new();
        let mut on_delta = |piece: &str| {
            deltas.push(piece.to_string());
            cancel.cancel();
        };
        let reply = read_stream(chunks, &mut on_delta, &cancel).await;
        assert_eq!(deltas, ["hello "]);
        assert_eq!(reply, Ok("hello ".into()));
    }

    #[tokio::test]
    async fn a_bare_think_tag_is_never_the_reply() {
        for pieces in [&["<think>"][..], &["<th", "ink>"][..], &[" \n", "<think>"][..]] {
            for (finish, message) in
                [("length", "output was cut off at the token limit"), ("stop", "no content in response")]
            {
                let events = sse(pieces, finish);
                for size in 1..=events.len() {
                    let (deltas, reply) = read(&events, size).await;
                    assert!(deltas.is_empty(), "{pieces:?} in chunks of {size} bytes");
                    assert_eq!(reply.unwrap_err().message, message, "{pieces:?} in chunks of {size} bytes");
                }
            }
        }
    }

    #[tokio::test]
    async fn a_streamed_reply_of_whitespace_alone_is_an_error() {
        for (finish, message) in
            [("length", "output was cut off at the token limit"), ("stop", "no content in response")]
        {
            let events = sse(&[" ", "\n "], finish);
            for size in 1..=events.len() {
                assert_eq!(read(&events, size).await.1.unwrap_err().message, message, "chunks of {size} bytes");
            }
        }
    }

    #[tokio::test]
    async fn a_stopped_stream_drops_what_it_was_still_holding() {
        let mut events = frame(json!({"role": "assistant", "content": ""}), Value::Null);
        events.push_str(&frame(json!({"content": "<thi"}), Value::Null));
        let cancel = ChatCancel::default();
        let stop = cancel.clone();
        let chunks = stream::once(async move { Ok(Some(events.into_bytes())) }).chain(stream::once(async move {
            stop.cancel();
            Ok(None)
        }));
        let mut deltas = Vec::new();
        let reply = read_stream(chunks, &mut |piece| deltas.push(piece.to_string()), &cancel).await;
        assert!(deltas.is_empty(), "{deltas:?}");
        assert_eq!(reply, Ok(String::new()));
    }
}
