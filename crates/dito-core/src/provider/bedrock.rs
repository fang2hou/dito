//! Amazon Bedrock through the Converse API. Credentials come from the user's AWS profile.

use async_trait::async_trait;
use aws_config::{BehaviorVersion, Region};
use aws_sdk_bedrockruntime::Client;
use aws_sdk_bedrockruntime::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_bedrockruntime::types::{
    ContentBlock, ConversationRole, InferenceConfiguration, Message, SystemContentBlock,
};

use super::{PolishRequest, Polisher};
use crate::error::{ErrorKind, ProviderError, ProviderResult};

#[derive(Clone)]
pub struct BedrockEngine {
    client: Client,
    model_id: String,
    reasoning_effort: String,
}

pub struct BedrockParams {
    pub profile: String,
    pub region: String,
    pub model_id: String,
    /// Test hook: point the SDK at a local mock instead of AWS.
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
        Ok(Self {
            client: Client::new(&conf),
            model_id: p.model_id.trim().to_string(),
            reasoning_effort: String::new(),
        })
    }

    pub fn with_reasoning_effort(mut self, value: &str) -> Self {
        self.reasoning_effort = value.trim().to_owned();
        self
    }

    async fn converse(&self, system: Option<&str>, content: ContentBlock) -> ProviderResult<String> {
        let msg = Message::builder()
            .role(ConversationRole::User)
            .content(content)
            .build()
            .map_err(|e| ProviderError::new(ErrorKind::Other, e.to_string()))?;
        let mut call = self
            .client
            .converse()
            .model_id(&self.model_id)
            .messages(msg)
            .inference_config(InferenceConfiguration::builder().max_tokens(2048).temperature(0.2).build());
        if let Some(s) = system {
            call = call.system(SystemContentBlock::Text(s.to_string()));
        }
        if !self.reasoning_effort.is_empty() {
            use aws_smithy_types::Document;
            let effort = Document::String(self.reasoning_effort.clone());
            // Bedrock's Claude effort is output_config.effort, not reasoning_effort.
            // https://docs.aws.amazon.com/bedrock/latest/userguide/claude-messages-adaptive-thinking.html
            let fields = if self.model_id.contains("anthropic") {
                call = call.inference_config(InferenceConfiguration::builder().max_tokens(2048).build());
                Document::Object(std::collections::HashMap::from([
                    (
                        "thinking".into(),
                        Document::Object(std::collections::HashMap::from([(
                            "type".into(),
                            Document::String("adaptive".into()),
                        )])),
                    ),
                    (
                        "output_config".into(),
                        Document::Object(std::collections::HashMap::from([("effort".into(), effort)])),
                    ),
                ]))
            } else {
                Document::Object(std::collections::HashMap::from([("reasoning_effort".into(), effort)]))
            };
            call = call.additional_model_request_fields(fields);
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
