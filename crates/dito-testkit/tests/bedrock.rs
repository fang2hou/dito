//! Bedrock Converse engine against the mock: polish, audio ASR, error classes.

use std::sync::Once;

use dito_core::audio::AudioClip;
use dito_core::error::ErrorKind;
use dito_core::provider::bedrock::{BedrockEngine, BedrockParams};
use dito_core::provider::{AsrRequest, PolishRequest, Polisher, Transcriber};
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
async fn polish_and_asr_use_converse_with_system_prompt_and_audio_block() {
    let srv = MockServer::start().await.unwrap();
    let e = engine(&srv.url, "apac.anthropic.claude-sonnet-4-20250514-v1:0").await;

    let out = e.polish(&PolishRequest { system: "be brief", text: "えーと 会議です" }).await.unwrap();
    assert_eq!(out, "会議です。");

    let clip = AudioClip::new(vec![0.3; 16_000]);
    let t = e.transcribe(&AsrRequest { clip: &clip, language: Some("ja"), hotwords: &["Acme".into()] }).await.unwrap();
    assert_eq!(t.text, "えーと 明日の3時に会議があります");

    let reqs = srv.requests();
    assert_eq!(reqs[0].system.as_deref(), Some("be brief"));
    assert!(
        reqs[0].model.as_deref().unwrap().contains("claude-sonnet-4"),
        "model id reaches the URL: {:?}",
        reqs[0].model
    );
    assert_eq!(reqs[1].audio_bytes, 1, "audio content block present");
    assert!(reqs[1].user.as_deref().unwrap().contains("Acme"));
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
