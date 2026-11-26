//! Amazon Bedrock through the Converse API. Credentials come from the user's AWS profile.

use std::collections::HashMap;

use async_trait::async_trait;
use aws_config::timeout::TimeoutConfig;
use aws_config::{BehaviorVersion, Region};
use aws_sdk_bedrockruntime::Client;
use aws_sdk_bedrockruntime::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_bedrockruntime::types::{
    ContentBlock, ContentBlockDelta, ConversationRole, ConverseStreamOutput, InferenceConfiguration, Message,
    StopReason, SystemContentBlock,
};
use aws_smithy_types::{Document, Number};
use futures_util::{Stream, StreamExt, stream};
use serde_json::Value;

use super::{
    ChatCancel, ChatDelta, ChatRequest, ChatRole, TextModel, TextRequest, Thinking, default_timeout, next_stream_item,
};
use crate::config::CloudTextParams;
use crate::error::{ErrorKind, ProviderError, ProviderResult};

#[derive(Clone)]
pub struct BedrockEngine {
    client: Client,
    model_id: String,
    params: CloudTextParams,
}

pub struct BedrockParams {
    pub profile: String,
    pub region: String,
    pub model_id: String,
    pub params: CloudTextParams,
    pub endpoint_override: Option<String>,
}

fn classify<E: ProvideErrorMetadata + std::fmt::Debug, R>(e: SdkError<E, R>) -> ProviderError {
    match &e {
        SdkError::ServiceError(s) => {
            let code = s.err().code().unwrap_or_default().to_string();
            let msg = s.err().message().unwrap_or_default().to_string();
            let kind = match code.as_str() {
                "AccessDeniedException"
                | "UnrecognizedClientException"
                | "ExpiredTokenException"
                | "InvalidSignatureException" => ErrorKind::Auth,
                "ThrottlingException" | "ServiceQuotaExceededException" => ErrorKind::RateLimit,
                "ResourceNotFoundException" => ErrorKind::ModelNotFound,
                "ModelTimeoutException" => ErrorKind::Timeout,
                "ValidationException" if msg.to_lowercase().contains("model") => ErrorKind::ModelNotFound,
                _ => ErrorKind::Other,
            };
            ProviderError::new(kind, format!("{code}: {msg}"))
        }
        SdkError::TimeoutError(_) => ProviderError::new(ErrorKind::Timeout, "request timed out"),
        SdkError::DispatchFailure(d) => {
            let text = format!("{d:?}");
            let kind = if text.to_lowercase().contains("credential") { ErrorKind::Auth } else { ErrorKind::Network };
            ProviderError::new(kind, text.chars().take(200).collect::<String>())
        }
        _ => ProviderError::new(ErrorKind::Other, e.to_string()),
    }
}

/// Claude's reasoning effort is `output_config.effort` next to adaptive thinking, not `reasoning_effort`.
/// <https://docs.aws.amazon.com/bedrock/latest/userguide/claude-messages-adaptive-thinking.html>
fn adaptive_thinking(model_id: &str, params: &CloudTextParams) -> bool {
    model_id.contains("anthropic") && !params.reasoning_effort.as_str().trim().is_empty()
}

/// Bedrock refuses temperature and top-p while adaptive thinking is on, so those are left out then.
fn inference_config(model_id: &str, params: &CloudTextParams) -> Option<InferenceConfiguration> {
    let sampling = !adaptive_thinking(model_id, params);
    let temperature = params.temperature.filter(|_| sampling);
    let top_p = params.top_p.filter(|_| sampling);
    let max_tokens = params.max_tokens.map(|limit| i32::try_from(limit).unwrap_or(i32::MAX));
    if temperature.is_none() && top_p.is_none() && max_tokens.is_none() {
        return None;
    }
    Some(
        InferenceConfiguration::builder()
            .set_temperature(temperature)
            .set_top_p(top_p)
            .set_max_tokens(max_tokens)
            .build(),
    )
}

/// Retrying would spend the same budget on the same cutoff.
fn polish_response(truncated: bool, text: String) -> ProviderResult<String> {
    if truncated {
        return Err(ProviderError::new(ErrorKind::BadResponse, "output was cut off at the token limit"));
    }
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(ProviderError::retryable(ErrorKind::BadResponse, "no text in Converse output"));
    }
    Ok(text)
}

fn additional_fields(model_id: &str, params: &CloudTextParams) -> Option<Document> {
    let text = |value: &str| Document::String(value.to_owned());
    let mut fields = HashMap::new();
    let effort = params.reasoning_effort.as_str().trim();
    if adaptive_thinking(model_id, params) {
        fields.insert("thinking".into(), Document::Object(HashMap::from([("type".into(), text("adaptive"))])));
        fields.insert("output_config".into(), Document::Object(HashMap::from([("effort".into(), text(effort))])));
    } else if !effort.is_empty() {
        fields.insert("reasoning_effort".into(), text(effort));
    }
    fields.extend(params.extra.iter().map(|(key, value)| (key.clone(), document(value))));
    (!fields.is_empty()).then_some(Document::Object(fields))
}

fn document(value: &Value) -> Document {
    match value {
        Value::Null => Document::Null,
        Value::Bool(flag) => Document::Bool(*flag),
        Value::Number(number) => Document::Number(match (number.as_u64(), number.as_i64()) {
            (Some(unsigned), _) => Number::PosInt(unsigned),
            (None, Some(signed)) => Number::NegInt(signed),
            (None, None) => Number::Float(number.as_f64().unwrap_or_default()),
        }),
        Value::String(text) => Document::String(text.clone()),
        Value::Array(items) => Document::Array(items.iter().map(document).collect()),
        Value::Object(fields) => {
            Document::Object(fields.iter().map(|(key, value)| (key.clone(), document(value))).collect())
        }
    }
}

impl BedrockEngine {
    pub async fn connect(p: BedrockParams) -> ProviderResult<Self> {
        if p.model_id.trim().is_empty() {
            return Err(ProviderError::new(ErrorKind::Unavailable, "model ID is not set"));
        }
        // The operation timeout spans every retry of a call and, for a stream, ends once the response headers
        // arrive; `next_stream_item` bounds the silence after that.
        let timeouts = TimeoutConfig::builder().operation_timeout(default_timeout()).build();
        let mut loader = aws_config::defaults(BehaviorVersion::latest()).timeout_config(timeouts);
        if !p.profile.trim().is_empty() {
            loader = loader.profile_name(p.profile.trim());
        }
        if !p.region.trim().is_empty() {
            loader = loader.region(Region::new(p.region.trim().to_string()));
        }
        if let Some(url) = &p.endpoint_override {
            loader = loader.endpoint_url(url);
        }
        let conf = loader.load().await;
        Ok(Self { client: Client::new(&conf), model_id: p.model_id.trim().to_string(), params: p.params })
    }

    async fn converse(&self, system: Option<&str>, turns: &[(ConversationRole, &str)]) -> ProviderResult<String> {
        let mut call = self.client.converse().model_id(&self.model_id);
        for (role, text) in turns {
            let message = Message::builder()
                .role(role.clone())
                .content(ContentBlock::Text((*text).to_string()))
                .build()
                .map_err(|e| ProviderError::new(ErrorKind::Other, e.to_string()))?;
            call = call.messages(message);
        }
        if let Some(config) = inference_config(&self.model_id, &self.params) {
            call = call.inference_config(config);
        }
        if let Some(fields) = additional_fields(&self.model_id, &self.params) {
            call = call.additional_model_request_fields(fields);
        }
        if let Some(s) = system {
            call = call.system(SystemContentBlock::Text(s.to_string()));
        }
        let out = call.send().await.map_err(classify)?;
        let text = out
            .output()
            .and_then(|o| o.as_message().ok())
            .map(|m| {
                m.content().iter().filter_map(|c| c.as_text().ok().map(String::as_str)).collect::<Vec<_>>().join("")
            })
            .unwrap_or_default();
        polish_response(matches!(out.stop_reason(), StopReason::MaxTokens), text)
    }

    /// The system messages go in apart from the turns.
    async fn converse_stream(
        &self,
        req: &ChatRequest,
        on_delta: ChatDelta<'_>,
        cancel: &ChatCancel,
    ) -> ProviderResult<String> {
        let params = CloudTextParams {
            temperature: req.temperature.or(self.params.temperature),
            top_p: req.top_p.or(self.params.top_p),
            max_tokens: req.max_tokens.or(self.params.max_tokens),
            reasoning_effort: match &req.thinking {
                Some(Thinking::Level(level)) => level.clone().into(),
                _ => self.params.reasoning_effort.clone(),
            },
            ..self.params.clone()
        };
        let mut call = self.client.converse_stream().model_id(&self.model_id);
        let mut turns: Vec<(ConversationRole, Vec<&str>)> = Vec::new();
        for m in &req.messages {
            let role = match m.role {
                ChatRole::System => {
                    call = call.system(SystemContentBlock::Text(m.content.clone()));
                    continue;
                }
                ChatRole::User => ConversationRole::User,
                ChatRole::Assistant => ConversationRole::Assistant,
            };
            merge_turn(&mut turns, role, &m.content);
        }
        for (role, texts) in turns {
            let message = texts
                .into_iter()
                .fold(Message::builder().role(role), |b, text| b.content(ContentBlock::Text(text.to_string())))
                .build()
                .map_err(|e| ProviderError::new(ErrorKind::Other, e.to_string()))?;
            call = call.messages(message);
        }
        if let Some(config) = inference_config(&self.model_id, &params) {
            call = call.inference_config(config);
        }
        if let Some(fields) = additional_fields(&self.model_id, &params) {
            call = call.additional_model_request_fields(fields);
        }
        let out = call.send().await.map_err(classify)?;
        read_reply(events(out), on_delta, cancel).await
    }
}

/// Consecutive turns of one role share a message, since Converse wants the roles to alternate.
fn merge_turn<'a>(turns: &mut Vec<(ConversationRole, Vec<&'a str>)>, role: ConversationRole, text: &'a str) {
    match turns.last_mut() {
        Some((last, texts)) if *last == role => texts.push(text),
        _ => turns.push((role, vec![text])),
    }
}

/// The events of a Converse stream as they arrive; an event of `None` ends it.
fn events(
    out: aws_sdk_bedrockruntime::operation::converse_stream::ConverseStreamOutput,
) -> impl Stream<Item = ProviderResult<Option<ConverseStreamOutput>>> {
    stream::unfold(out.stream, |mut stream| async move { Some((stream.recv().await.map_err(classify), stream)) })
}

/// Reads a Converse stream into its reply. A reply that ends at the token limit is returned as it stands, since its
/// text is already on screen; one with no text at all is an error that names the limit when the model spent its
/// whole budget there.
async fn read_reply(
    events: impl Stream<Item = ProviderResult<Option<ConverseStreamOutput>>>,
    on_delta: ChatDelta<'_>,
    cancel: &ChatCancel,
) -> ProviderResult<String> {
    let mut events = std::pin::pin!(events);
    let (mut reply, mut cut_off) = (String::new(), false);
    while !cancel.is_cancelled() {
        let next = async { events.next().await.unwrap_or(Ok(None)) };
        let Some(event) = next_stream_item(next, cancel).await? else { break };
        match event {
            // Reasoning deltas of adaptive thinking are not part of the reply.
            ConverseStreamOutput::ContentBlockDelta(delta) => {
                if let Some(ContentBlockDelta::Text(piece)) = delta.delta()
                    && !piece.is_empty()
                {
                    on_delta(piece);
                    reply.push_str(piece);
                }
            }
            ConverseStreamOutput::MessageStop(stop) => cut_off = stop.stop_reason() == &StopReason::MaxTokens,
            _ => {}
        }
    }
    if reply.trim().is_empty() && !cancel.is_cancelled() {
        return Err(if cut_off {
            ProviderError::new(ErrorKind::BadResponse, "output was cut off at the token limit")
        } else {
            ProviderError::new(ErrorKind::BadResponse, "no text in Converse output")
        });
    }
    Ok(reply)
}

#[async_trait]
impl TextModel for BedrockEngine {
    async fn polish(&self, req: &TextRequest<'_>) -> ProviderResult<String> {
        let mut turns = Vec::with_capacity(req.examples.len() * 2 + 1);
        for example in req.examples {
            turns.push((ConversationRole::User, example.said));
            turns.push((ConversationRole::Assistant, example.written));
        }
        turns.push((ConversationRole::User, req.text));
        self.converse(Some(req.system), &turns).await
    }

    /// Streams through ConverseStream. A reply that stops at the token limit is returned as it stands, since its text
    /// is already on screen; one with no text at all is an error.
    async fn chat(&self, req: &ChatRequest, on_delta: ChatDelta<'_>, cancel: &ChatCancel) -> ProviderResult<String> {
        if cancel.is_cancelled() {
            return Ok(String::new());
        }
        self.converse_stream(req, on_delta, cancel).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ReasoningEffort;
    use aws_sdk_bedrockruntime::types::{ContentBlockDeltaEvent, MessageStopEvent};

    const CLAUDE: &str = "apac.anthropic.claude-sonnet-4-20250514-v1:0";
    const OTHER: &str = "openai.gpt-oss-120b-1:0";

    fn params(effort: &str) -> CloudTextParams {
        CloudTextParams {
            temperature: Some(0.2),
            top_p: Some(0.9),
            max_tokens: Some(2048),
            reasoning_effort: ReasoningEffort::from(effort.to_owned()),
            ..CloudTextParams::default()
        }
    }

    #[test]
    fn only_empty_polish_output_is_retryable() {
        // The same prompt and the same budget are cut at the same place again.
        let cut_off = polish_response(true, "partial".into()).unwrap_err();
        assert_eq!((cut_off.kind, cut_off.is_retryable()), (ErrorKind::BadResponse, false));
        assert_eq!(cut_off.message, "output was cut off at the token limit");
        assert!(polish_response(false, "  ".into()).unwrap_err().is_retryable());
        assert_eq!(polish_response(false, "done".into()), Ok("done".into()));
    }

    #[test]
    fn consecutive_turns_of_one_role_share_a_message() {
        let mut turns = Vec::new();
        for (role, text) in [
            (ConversationRole::User, "a"),
            (ConversationRole::User, "b"),
            (ConversationRole::Assistant, "c"),
            (ConversationRole::Assistant, "d"),
            (ConversationRole::User, "e"),
        ] {
            merge_turn(&mut turns, role, text);
        }
        assert_eq!(
            turns,
            [
                (ConversationRole::User, vec!["a", "b"]),
                (ConversationRole::Assistant, vec!["c", "d"]),
                (ConversationRole::User, vec!["e"]),
            ]
        );
    }

    /// Feeds the events of one stream to the reader: the deltas shown, then the reply returned.
    async fn read(events: Vec<ConverseStreamOutput>) -> (Vec<String>, ProviderResult<String>) {
        let mut deltas = Vec::new();
        let events = stream::iter(events).map(|event| Ok(Some(event)));
        let reply = read_reply(events, &mut |piece| deltas.push(piece.to_string()), &ChatCancel::default()).await;
        (deltas, reply)
    }

    #[tokio::test]
    async fn a_reply_cut_off_before_its_first_word_is_an_error_that_names_the_token_limit() {
        let text = |piece: &str| {
            ConverseStreamOutput::ContentBlockDelta(
                ContentBlockDeltaEvent::builder()
                    .content_block_index(0)
                    .delta(ContentBlockDelta::Text(piece.into()))
                    .build()
                    .unwrap(),
            )
        };
        let stop = |reason: StopReason| {
            ConverseStreamOutput::MessageStop(MessageStopEvent::builder().stop_reason(reason).build().unwrap())
        };

        // Text already on screen is kept when the limit ends the reply.
        let (deltas, reply) = read(vec![text("hel"), text("lo"), stop(StopReason::MaxTokens)]).await;
        assert_eq!(deltas.concat(), "hello");
        assert_eq!(reply, Ok("hello".into()));

        let (deltas, reply) = read(vec![stop(StopReason::MaxTokens)]).await;
        assert!(deltas.is_empty());
        let error = reply.unwrap_err();
        assert_eq!((error.kind, error.is_retryable()), (ErrorKind::BadResponse, false));
        assert_eq!(error.message, "output was cut off at the token limit");

        // Whitespace alone is no text, at the token limit or not.
        let (_, reply) = read(vec![text(" \n "), stop(StopReason::MaxTokens)]).await;
        assert_eq!(reply.unwrap_err().message, "output was cut off at the token limit");
        let (_, reply) = read(vec![text(" \n "), stop(StopReason::EndTurn)]).await;
        assert_eq!(reply.unwrap_err().message, "no text in Converse output");
        let (_, reply) = read(vec![stop(StopReason::EndTurn)]).await;
        assert_eq!(reply.unwrap_err().message, "no text in Converse output");
    }

    #[test]
    fn an_unset_parameter_is_not_in_the_inference_configuration() {
        let unset = CloudTextParams { reasoning_effort: ReasoningEffort::Custom(String::new()), ..Default::default() };
        assert_eq!(inference_config(CLAUDE, &unset), None, "no empty configuration is sent");
        let tokens_only = CloudTextParams { max_tokens: Some(64), ..unset };
        let config = inference_config(CLAUDE, &tokens_only).unwrap();
        assert_eq!((config.max_tokens(), config.temperature(), config.top_p()), (Some(64), None, None));
    }

    #[test]
    fn adaptive_thinking_leaves_out_what_bedrock_refuses_alongside_it() {
        let thinking = inference_config(CLAUDE, &params("low")).unwrap();
        assert_eq!((thinking.max_tokens(), thinking.temperature(), thinking.top_p()), (Some(2048), None, None));
        let plain = inference_config(CLAUDE, &params("")).unwrap();
        assert_eq!((plain.max_tokens(), plain.temperature(), plain.top_p()), (Some(2048), Some(0.2), Some(0.9)));
        let other_family = inference_config(OTHER, &params("high")).unwrap();
        assert_eq!(other_family.temperature(), Some(0.2), "only Claude switches to adaptive thinking");
    }

    #[test]
    fn the_reasoning_effort_takes_the_form_of_the_model_family_and_extra_goes_over_it() {
        assert_eq!(additional_fields(CLAUDE, &params("")), None);
        let claude = additional_fields(CLAUDE, &params(" low ")).unwrap();
        let expected =
            document(&serde_json::json!({"thinking": {"type": "adaptive"}, "output_config": {"effort": "low"}}));
        assert_eq!(claude, expected);
        let other = additional_fields(OTHER, &params("high")).unwrap();
        assert_eq!(other, document(&serde_json::json!({"reasoning_effort": "high"})));

        let extra = CloudTextParams {
            extra: serde_json::from_str(r#"{"reasoning_effort": "x", "top_k": 5, "stop": [null, 1.5, -2, true]}"#)
                .unwrap(),
            ..params("high")
        };
        let merged = additional_fields(OTHER, &extra).unwrap();
        let expected =
            document(&serde_json::json!({"reasoning_effort": "x", "top_k": 5, "stop": [null, 1.5, -2, true]}));
        assert_eq!(merged, expected);
    }
}
