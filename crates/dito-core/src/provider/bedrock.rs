//! Amazon Bedrock through the Converse API. Credentials come from the user's AWS profile.

use std::collections::HashMap;

use async_trait::async_trait;
use aws_config::{BehaviorVersion, Region};
use aws_sdk_bedrockruntime::Client;
use aws_sdk_bedrockruntime::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_bedrockruntime::types::{
    ContentBlock, ConversationRole, InferenceConfiguration, Message, SystemContentBlock,
};
use aws_smithy_types::{Document, Number};
use serde_json::Value;

use super::{PolishRequest, Polisher};
use crate::config::CloudPolishParams;
use crate::error::{ErrorKind, ProviderError, ProviderResult};

#[derive(Clone)]
pub struct BedrockEngine {
    client: Client,
    model_id: String,
    params: CloudPolishParams,
}

pub struct BedrockParams {
    pub profile: String,
    pub region: String,
    pub model_id: String,
    pub params: CloudPolishParams,
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
fn adaptive_thinking(model_id: &str, params: &CloudPolishParams) -> bool {
    model_id.contains("anthropic") && !params.reasoning_effort.as_str().trim().is_empty()
}

/// Bedrock refuses temperature and top-p while adaptive thinking is on, so those are left out then.
fn inference_config(model_id: &str, params: &CloudPolishParams) -> Option<InferenceConfiguration> {
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

fn additional_fields(model_id: &str, params: &CloudPolishParams) -> Option<Document> {
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
        let mut loader = aws_config::defaults(BehaviorVersion::latest());
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

    async fn converse(&self, system: Option<&str>, content: ContentBlock) -> ProviderResult<String> {
        let msg = Message::builder()
            .role(ConversationRole::User)
            .content(content)
            .build()
            .map_err(|e| ProviderError::new(ErrorKind::Other, e.to_string()))?;
        let mut call = self.client.converse().model_id(&self.model_id).messages(msg);
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
        if matches!(out.stop_reason(), aws_sdk_bedrockruntime::types::StopReason::MaxTokens) {
            return Err(ProviderError::new(ErrorKind::BadResponse, "output was cut off at the token limit"));
        }
        let text = out
            .output()
            .and_then(|o| o.as_message().ok())
            .map(|m| {
                m.content().iter().filter_map(|c| c.as_text().ok().map(String::as_str)).collect::<Vec<_>>().join("")
            })
            .unwrap_or_default();
        let text = text.trim().to_string();
        if text.is_empty() {
            return Err(ProviderError::new(ErrorKind::BadResponse, "no text in Converse output"));
        }
        Ok(text)
    }
}

#[async_trait]
impl Polisher for BedrockEngine {
    async fn polish(&self, req: &PolishRequest<'_>) -> ProviderResult<String> {
        self.converse(Some(req.system), ContentBlock::Text(req.text.to_string())).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ReasoningEffort;

    const CLAUDE: &str = "apac.anthropic.claude-sonnet-4-20250514-v1:0";
    const OTHER: &str = "openai.gpt-oss-120b-1:0";

    fn params(effort: &str) -> CloudPolishParams {
        CloudPolishParams {
            temperature: Some(0.2),
            top_p: Some(0.9),
            max_tokens: Some(2048),
            reasoning_effort: ReasoningEffort::from(effort.to_owned()),
            ..CloudPolishParams::default()
        }
    }

    #[test]
    fn an_unset_parameter_is_not_in_the_inference_configuration() {
        let unset =
            CloudPolishParams { reasoning_effort: ReasoningEffort::Custom(String::new()), ..Default::default() };
        assert_eq!(inference_config(CLAUDE, &unset), None, "no empty configuration is sent");
        let tokens_only = CloudPolishParams { max_tokens: Some(64), ..unset };
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

        let extra = CloudPolishParams {
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
