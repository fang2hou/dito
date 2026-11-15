//! Bedrock Converse engine against the mock.

use std::sync::Once;

use dito_core::config::{CloudTextParams, CloudTextTemplate, ReasoningEffort};
use dito_core::error::ErrorKind;
use dito_core::provider::bedrock::{BedrockEngine, BedrockParams};
use dito_core::provider::{TextModel, TextRequest};
use dito_testkit::MockServer;
use serde_json::{Value, json};

const CLAUDE: &str = "apac.anthropic.claude-sonnet-4-20250514-v1:0";
const OTHER: &str = "openai.gpt-oss-120b-1:0";

fn creds() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: `Once` runs this before the first `BedrockEngine::connect` in any test, and the only code
        // in this binary that reads the environment is the AWS credential chain inside `connect`, so no
        // thread reads the environment while it is written.
        unsafe {
            std::env::set_var("AWS_ACCESS_KEY_ID", "AKIDEXAMPLE");
            std::env::set_var("AWS_SECRET_ACCESS_KEY", "secret");
            std::env::set_var("AWS_EC2_METADATA_DISABLED", "true");
        }
    });
}

async fn engine_with(url: &str, model: &str, params: CloudTextParams) -> BedrockEngine {
    creds();
    BedrockEngine::connect(BedrockParams {
        profile: String::new(),
        region: "ap-northeast-1".into(),
        model_id: model.into(),
        params,
        endpoint_override: Some(url.into()),
    })
    .await
    .unwrap()
}

async fn engine(url: &str, model: &str) -> BedrockEngine {
    engine_with(
        url,
        model,
        CloudTextParams { reasoning_effort: ReasoningEffort::Custom(String::new()), ..Default::default() },
    )
    .await
}

async fn sent(model: &str, params: CloudTextParams) -> Value {
    let srv = MockServer::start().await.unwrap();
    let e = engine_with(&srv.url, model, params).await;
    e.polish(&TextRequest { system: "s", examples: &[], text: "ok" }).await.unwrap();
    srv.requests()[0].body.clone().expect("the Converse route records its body")
}

fn approx(value: &Value, expected: f64) -> bool {
    value.as_f64().is_some_and(|v| (v - expected).abs() < 1e-6)
}

fn template_params() -> CloudTextParams {
    CloudTextTemplate::Bedrock.configuration("bedrock", "Amazon Bedrock").params
}

#[tokio::test]
async fn polish_uses_converse_with_the_system_prompt() {
    let srv = MockServer::start().await.unwrap();
    let e = engine(&srv.url, CLAUDE).await;

    let out = e.polish(&TextRequest { system: "be brief", examples: &[], text: "えーと 会議です" }).await.unwrap();
    assert_eq!(out, "会議です。");

    let reqs = srv.requests();
    assert_eq!(reqs[0].system.as_deref(), Some("be brief"));
    assert!(
        reqs[0].model.as_deref().unwrap().contains("claude-sonnet-4"),
        "model id reaches the URL: {:?}",
        reqs[0].model
    );
}

#[tokio::test]
async fn nothing_is_sent_for_parameters_that_are_not_set() {
    let body = sent(
        CLAUDE,
        CloudTextParams { reasoning_effort: ReasoningEffort::Custom(String::new()), ..Default::default() },
    )
    .await;
    assert!(body.get("inferenceConfig").is_none(), "{body}");
    assert!(body.get("additionalModelRequestFields").is_none(), "{body}");
}

#[tokio::test]
async fn the_template_starts_with_the_sampling_the_engine_always_sent() {
    let body = sent(CLAUDE, template_params()).await;
    let config = &body["inferenceConfig"];
    assert_eq!(config["maxTokens"], json!(2048));
    assert!(approx(&config["temperature"], 0.2), "{config}");
    assert!(config.get("topP").is_none(), "{config}");
    assert!(body.get("additionalModelRequestFields").is_none(), "no reasoning effort by default: {body}");
}

#[tokio::test]
async fn claude_with_an_effort_thinks_adaptively_and_is_sent_no_sampling() {
    let params = CloudTextParams { top_p: Some(0.9), reasoning_effort: ReasoningEffort::Low, ..template_params() };
    let body = sent(CLAUDE, params).await;
    assert_eq!(body["inferenceConfig"], json!({"maxTokens": 2048}), "Bedrock refuses sampling next to thinking");
    assert_eq!(
        body["additionalModelRequestFields"],
        json!({"thinking": {"type": "adaptive"}, "output_config": {"effort": "low"}})
    );
}

#[tokio::test]
async fn other_models_get_the_effort_as_reasoning_effort_and_keep_their_sampling() {
    let params = CloudTextParams { top_p: Some(0.9), reasoning_effort: ReasoningEffort::High, ..template_params() };
    let body = sent(OTHER, params).await;
    let config = &body["inferenceConfig"];
    assert!(approx(&config["temperature"], 0.2) && approx(&config["topP"], 0.9), "{config}");
    assert_eq!(body["additionalModelRequestFields"], json!({"reasoning_effort": "high"}));
}

#[tokio::test]
async fn extra_goes_into_the_additional_model_request_fields_over_the_effort() {
    let params = CloudTextParams {
        reasoning_effort: ReasoningEffort::Low,
        extra: serde_json::from_value(json!({"top_k": 5, "output_config": {"effort": "high"}, "stop": [null, 1.5]}))
            .unwrap(),
        ..template_params()
    };
    let body = sent(CLAUDE, params).await;
    assert_eq!(
        body["additionalModelRequestFields"],
        json!({"thinking": {"type": "adaptive"}, "output_config": {"effort": "high"}, "top_k": 5, "stop": [null, 1.5]})
    );
}

#[tokio::test]
async fn service_errors_are_classified() {
    let srv = MockServer::start().await.unwrap();
    let e = engine(&srv.url, "m").await;
    for (status, kind) in [(403, ErrorKind::Auth), (429, ErrorKind::RateLimit), (404, ErrorKind::ModelNotFound)] {
        srv.behavior(|b| b.fail_status = Some(status));
        let err = e.polish(&TextRequest { system: "s", examples: &[], text: "t" }).await.unwrap_err();
        assert_eq!(err.kind, kind, "status {status}: {}", err.message);
    }
}

#[tokio::test]
async fn empty_model_id_is_rejected_up_front() {
    let err = BedrockEngine::connect(BedrockParams {
        profile: String::new(),
        region: "ap-northeast-1".into(),
        model_id: "  ".into(),
        params: CloudTextParams::default(),
        endpoint_override: None,
    })
    .await
    .err()
    .unwrap();
    assert_eq!(err.kind, ErrorKind::Unavailable);
}
