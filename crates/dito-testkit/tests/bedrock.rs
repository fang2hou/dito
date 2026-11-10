//! Bedrock Converse engine against the mock: polish and error classes.

use std::sync::Once;

use dito_core::error::ErrorKind;
use dito_core::provider::bedrock::{BedrockEngine, BedrockParams};
use dito_core::provider::{PolishRequest, Polisher};
use dito_testkit::MockServer;

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

async fn engine(url: &str, model: &str) -> BedrockEngine {
    creds();
    BedrockEngine::connect(BedrockParams {
        profile: String::new(),
        region: "ap-northeast-1".into(),
        model_id: model.into(),
        endpoint_override: Some(url.into()),
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn polish_uses_converse_with_the_system_prompt() {
    let srv = MockServer::start().await.unwrap();
    let e = engine(&srv.url, "apac.anthropic.claude-sonnet-4-20250514-v1:0").await;

    let out = e.polish(&PolishRequest { system: "be brief", text: "えーと 会議です" }).await.unwrap();
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
async fn service_errors_are_classified() {
    let srv = MockServer::start().await.unwrap();
    let e = engine(&srv.url, "m").await;
    for (status, kind) in [(403, ErrorKind::Auth), (429, ErrorKind::RateLimit), (404, ErrorKind::ModelNotFound)] {
        srv.behavior(|b| b.fail_status = Some(status));
        let err = e.polish(&PolishRequest { system: "s", text: "t" }).await.unwrap_err();
        assert_eq!(err.kind, kind, "status {status}: {}", err.message);
    }
}

#[tokio::test]
async fn empty_model_id_is_rejected_up_front() {
    let err = BedrockEngine::connect(BedrockParams {
        profile: String::new(),
        region: "ap-northeast-1".into(),
        model_id: "  ".into(),
        endpoint_override: None,
    })
    .await
    .err()
    .unwrap();
    assert_eq!(err.kind, ErrorKind::Unavailable);
}
