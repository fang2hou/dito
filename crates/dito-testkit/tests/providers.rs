//! Cloud speech and polish engines against the mock server.

use std::sync::Arc;
use std::time::Duration;

use dito_core::audio::AudioClip;
use dito_core::config::{
    CloudAsrConfigurations, CloudPolishConfiguration, CloudPolishParams, Config, ReasoningEffort, Source,
};
use dito_core::error::ErrorKind;
use dito_core::factory::{EngineFactory, Engines};
use dito_core::pipeline::CancellationToken;
use dito_core::pipeline::{self, PipelineError, PipelineJob};
use dito_core::prompts::Style;
use dito_core::provider::{AsrRequest, PolishRequest, TestOutcome, test_polisher, test_transcriber};
use dito_core::secrets::{MemorySecrets, SecretStore, polish_account};
use dito_testkit::{MockServer, mock_cloud_asr_config};
use serde_json::json;

fn tone() -> AudioClip {
    AudioClip::new(vec![0.3; 16_000])
}

fn active(cfg: &mut Config) -> &mut CloudPolishConfiguration {
    cfg.polish.engine.polish_configurations_mut(cfg.general.lang()).active_mut().expect("a configuration is selected")
}

fn mock_cfg(url: &str) -> Config {
    let mut c = Config::dev_azure();
    let configurations = c.asr.cloud_asr.as_mut().expect("the dev defaults use a cloud speech configuration");
    *configurations.active_mut().expect("the selected configuration is in the list") = mock_cloud_asr_config(url);
    *active(&mut c).target.address_mut() = url.into();
    c
}

fn compat_cfg(base_url: &str) -> Config {
    let mut cfg = Config::default();
    cfg.polish.engine.source = Source::Provider;
    cfg.polish.engine.polish_configurations_mut(cfg.general.lang()).select("compat");
    *active(&mut cfg).target.address_mut() = base_url.into();
    *active(&mut cfg).target.model_mut() = "gpt-4o-mini".into();
    cfg
}

fn factory(key: Option<&str>) -> EngineFactory {
    let secrets = Arc::new(MemorySecrets::default());
    if let Some(k) = key {
        secrets.set(&CloudAsrConfigurations::default().active().unwrap().account(), k).unwrap();
        secrets.set(&polish_account("azure"), k).unwrap();
        secrets.set(&polish_account("compat"), k).unwrap();
    }
    EngineFactory::new(secrets)
}

#[tokio::test]
async fn cloud_asr_and_azure_polish_hit_their_routes_with_models_and_key() {
    let srv = MockServer::start().await.unwrap();
    srv.behavior(|b| b.expected_key = Some("secret".into()));
    let cfg = mock_cfg(&srv.url);
    let f = factory(Some("secret"));

    let asr = f.transcriber(&cfg).await.unwrap();
    let clip = tone();
    let t = asr.transcribe(&AsrRequest { clip: &clip, language: Some("ja"), hotwords: &[] }).await.unwrap();
    assert_eq!(t.text, "えーと 明日の3時に会議があります");
    assert_eq!(t.language.as_deref(), Some("ja"));

    let polish = f.polisher(&cfg).await.unwrap().unwrap();
    let out = polish.polish(&PolishRequest { system: "sys", examples: &[], text: "えーと 会議です" }).await.unwrap();
    assert_eq!(out, "会議です。");

    let reqs = srv.requests();
    assert_eq!(reqs[0].path, "audio/transcriptions");
    assert!(reqs[0].headers.iter().any(|(k, v)| k == "authorization" && v == "Bearer secret"));
    assert_eq!(reqs[0].model.as_deref(), Some("mock-asr"));
    assert_eq!(reqs[0].language.as_deref(), Some("ja"));
    assert_eq!(reqs[0].audio_bytes, 44 + 32_000, "WAV header + 1 s of 16-bit samples");
    assert_eq!(reqs[1].model.as_deref(), Some("gpt-6.1-terra"));
    assert_eq!(reqs[1].reasoning_effort.as_deref(), Some("low"));
    assert_eq!(reqs[1].system.as_deref(), Some("sys"));
}

#[tokio::test]
async fn wrong_key_is_classified_as_auth_for_both_engines() {
    let srv = MockServer::start().await.unwrap();
    srv.behavior(|b| b.expected_key = Some("right".into()));
    let cfg = mock_cfg(&srv.url);
    let f = factory(Some("wrong"));
    match test_transcriber(f.transcriber(&cfg).await.unwrap().as_ref()).await {
        TestOutcome::Failed { kind, .. } => assert_eq!(kind, ErrorKind::Auth),
        other => panic!("{other:?}"),
    }
    match test_polisher(f.polisher(&cfg).await.unwrap().unwrap().as_ref()).await {
        TestOutcome::Failed { kind, .. } => assert_eq!(kind, ErrorKind::Auth),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn http_statuses_map_to_error_kinds() {
    let srv = MockServer::start().await.unwrap();
    let cfg = mock_cfg(&srv.url);
    let f = factory(None);
    for (status, kind) in [(429, ErrorKind::RateLimit), (404, ErrorKind::ModelNotFound), (500, ErrorKind::Other)] {
        srv.behavior(|b| b.fail_status = Some(status));
        let engine = f.polisher(&cfg).await.unwrap().unwrap();
        let err = engine.polish(&PolishRequest { system: "s", examples: &[], text: "t" }).await.unwrap_err();
        assert_eq!(err.kind, kind, "status {status}");
    }
}

#[tokio::test]
async fn a_server_that_hangs_up_is_a_network_error_not_a_hang() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let hang_up = tokio::spawn(async move {
        loop {
            let _ = listener.accept().await;
        }
    });
    let f = factory(None);
    let engine = f.polisher(&mock_cfg(&url)).await.unwrap().unwrap();
    let err = tokio::time::timeout(
        Duration::from_secs(15),
        engine.polish(&PolishRequest { system: "s", examples: &[], text: "t" }),
    )
    .await
    .expect("must fail fast")
    .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Network);
    hang_up.abort();
}

#[tokio::test]
async fn connection_test_reports_latency_and_treats_no_speech_as_reachable() {
    let srv = MockServer::start().await.unwrap();
    srv.behavior(|b| {
        b.latency = Duration::from_millis(5);
        b.transcript = String::new();
    });
    let f = factory(None);
    let asr = f.transcriber(&mock_cfg(&srv.url)).await.unwrap();
    match test_transcriber(asr.as_ref()).await {
        TestOutcome::Ok { latency_ms } => assert!(latency_ms >= 5, "latency {latency_ms}"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn compat_accepts_a_pasted_chat_completions_url_and_uses_bearer_auth() {
    let srv = MockServer::start().await.unwrap();
    srv.behavior(|b| b.expected_key = Some("tok".into()));
    let cfg = compat_cfg(&format!("{}/v1/chat/completions", srv.url));
    let f = factory(Some("tok"));
    let p = f.polisher(&cfg).await.unwrap().unwrap();
    assert_eq!(p.polish(&PolishRequest { system: "s", examples: &[], text: "um hello" }).await.unwrap(), "hello.");
    let r = &srv.requests()[0];
    assert!(r.headers.iter().any(|(k, v)| k == "authorization" && v == "Bearer tok"));
}

#[tokio::test]
async fn reasoning_effort_is_dropped_and_retried_for_models_that_reject_it() {
    let srv = MockServer::start().await.unwrap();
    srv.behavior(|b| b.reject_reasoning_effort = true);
    let mut cfg = mock_cfg(&srv.url);
    active(&mut cfg).params.reasoning_effort = ReasoningEffort::High;
    active(&mut cfg).params.temperature = Some(0.3);
    let p = factory(None).polisher(&cfg).await.unwrap().unwrap();
    assert_eq!(p.polish(&PolishRequest { system: "s", examples: &[], text: "ok" }).await.unwrap(), "ok.");
    let reqs = srv.requests();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0].reasoning_effort.as_deref(), Some("high"));
    assert_eq!(reqs[1].reasoning_effort, None);
    assert_eq!(reqs[1].body.as_ref().unwrap()["temperature"], json!(0.3), "the retry keeps the other parameters");
}

fn body_keys(body: &serde_json::Value) -> Vec<&str> {
    let mut keys: Vec<_> = body.as_object().expect("a JSON object").keys().map(String::as_str).collect();
    keys.sort_unstable();
    keys
}

async fn polish_body(cfg: &Config, srv: &MockServer) -> serde_json::Value {
    let before = srv.requests().len();
    let p = factory(None).polisher(cfg).await.unwrap().unwrap();
    p.polish(&PolishRequest { system: "s", examples: &[], text: "ok" }).await.unwrap();
    srv.requests()[before].body.clone().expect("the chat route records its body")
}

#[tokio::test]
async fn a_parameter_that_is_not_set_is_not_sent() {
    let srv = MockServer::start().await.unwrap();
    for mut cfg in [mock_cfg(&srv.url), compat_cfg(&format!("{}/v1", srv.url))] {
        active(&mut cfg).params =
            CloudPolishParams { reasoning_effort: ReasoningEffort::Custom(String::new()), ..Default::default() };
        let body = polish_body(&cfg, &srv).await;
        assert_eq!(body_keys(&body), ["messages", "model"], "nothing but the prompt and the model");
    }
}

#[tokio::test]
async fn set_parameters_are_sent_under_the_token_limit_name_of_their_api() {
    let srv = MockServer::start().await.unwrap();
    let params = CloudPolishParams {
        temperature: Some(0.3),
        top_p: Some(0.9),
        max_tokens: Some(256),
        reasoning_effort: ReasoningEffort::Medium,
        ..Default::default()
    };

    let mut azure = mock_cfg(&srv.url);
    active(&mut azure).params = params.clone();
    let body = polish_body(&azure, &srv).await;
    assert_eq!(
        body_keys(&body),
        ["max_completion_tokens", "messages", "model", "reasoning_effort", "temperature", "top_p"]
    );
    assert_eq!(
        (&body["temperature"], &body["top_p"], &body["max_completion_tokens"], &body["reasoning_effort"]),
        (&json!(0.3), &json!(0.9), &json!(256), &json!("medium"))
    );

    let mut compat = compat_cfg(&format!("{}/v1", srv.url));
    active(&mut compat).params = params;
    let body = polish_body(&compat, &srv).await;
    assert_eq!(body_keys(&body), ["max_tokens", "messages", "model", "reasoning_effort", "temperature", "top_p"]);
    assert_eq!(body["max_tokens"], json!(256));
}

#[tokio::test]
async fn extra_is_merged_over_the_request_body_last() {
    let srv = MockServer::start().await.unwrap();
    let mut cfg = mock_cfg(&srv.url);
    active(&mut cfg).params = CloudPolishParams {
        temperature: Some(0.3),
        extra: serde_json::from_value(json!({"temperature": 0.9, "seed": 42, "response_format": {"type": "text"}}))
            .unwrap(),
        ..Default::default()
    };
    let body = polish_body(&cfg, &srv).await;
    assert_eq!(body["temperature"], json!(0.9), "extra overrides a parameter with a field of its own");
    assert_eq!((&body["seed"], &body["response_format"]), (&json!(42), &json!({"type": "text"})));
    assert_eq!(body["model"], json!("gpt-6.1-terra"), "the rest of the body stays");
}

#[tokio::test]
async fn every_configuration_authenticates_with_the_key_under_its_own_account() {
    let srv = MockServer::start().await.unwrap();
    srv.behavior(|b| b.expected_key = Some("key-of-the-copy".into()));
    let secrets = Arc::new(MemorySecrets::default());
    let f = EngineFactory::new(secrets.clone());
    let mut cfg = mock_cfg(&srv.url);
    let original = active(&mut cfg).id.clone();
    let configurations = cfg.polish.engine.polish_configurations_mut(cfg.general.lang());
    let copy = configurations.duplicate(&original, "Copy").unwrap();
    secrets.set(&polish_account(&original), "key-of-the-original").unwrap();
    assert_eq!(secrets.get(&polish_account(&copy)), None, "a copy starts without a key");
    secrets.set(&polish_account(&copy), "key-of-the-copy").unwrap();
    let request = PolishRequest { system: "s", examples: &[], text: "ok" };

    f.polisher(&cfg).await.unwrap().unwrap().polish(&request).await.unwrap();
    cfg.polish.engine.polish_configurations_mut(cfg.general.lang()).select(&original);
    let err = f.polisher(&cfg).await.unwrap().unwrap().polish(&request).await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::Auth, "the original sends its own key");
}

#[tokio::test]
async fn a_changed_key_is_picked_up_once_the_engines_are_invalidated() {
    let srv = MockServer::start().await.unwrap();
    srv.behavior(|b| b.expected_key = Some("new".into()));
    let secrets = Arc::new(MemorySecrets::default());
    let f = EngineFactory::new(secrets.clone());
    let cfg = mock_cfg(&srv.url);
    let account = polish_account("azure");
    let request = PolishRequest { system: "s", examples: &[], text: "ok" };

    secrets.set(&account, "old").unwrap();
    let first = f.polisher(&cfg).await.unwrap().unwrap().polish(&request).await.unwrap_err();
    assert_eq!(first.kind, ErrorKind::Auth);
    secrets.set(&account, "new").unwrap();
    f.invalidate_engines();
    f.polisher(&cfg).await.unwrap().unwrap().polish(&request).await.unwrap();
}

#[tokio::test]
async fn a_polish_reply_cut_off_at_the_token_limit_is_a_bad_response() {
    let srv = MockServer::start().await.unwrap();
    srv.behavior(|b| b.truncate_chat = true);
    let cfg = mock_cfg(&srv.url);
    let p = factory(None).polisher(&cfg).await.unwrap().unwrap();
    let err = p.polish(&PolishRequest { system: "s", examples: &[], text: "ok" }).await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::BadResponse);
}

#[tokio::test]
async fn a_blank_base_url_or_model_fails_before_any_request() {
    let srv = MockServer::start().await.unwrap();
    for field in ["base_url", "model"] {
        let mut cfg = mock_cfg(&srv.url);
        let config = cfg.asr.cloud_asr.as_mut().unwrap().active_mut().unwrap();
        config["connection"][field] = "".into();
        let err = factory(None).transcriber(&cfg).await.err().unwrap();
        assert_eq!(err.kind, ErrorKind::Unavailable, "blank {field}");
    }
    assert!(srv.requests().is_empty());
}

#[tokio::test]
async fn full_pipeline_over_http_with_polish_outage_still_delivers_raw_text() {
    let srv = MockServer::start().await.unwrap();
    let cfg = mock_cfg(&srv.url);
    let f = factory(None);
    // ASR healthy; flip the server to fail only after the transcription has been served.
    struct Flip<'a>(&'a MockServer, EngineFactory);
    #[async_trait::async_trait]
    impl Engines for Flip<'_> {
        async fn transcriber(
            &self,
            c: &Config,
        ) -> dito_core::error::ProviderResult<Arc<dyn dito_core::provider::Transcriber>> {
            let t = self.1.transcriber(c).await?;
            Ok(t)
        }
        async fn polisher(
            &self,
            c: &Config,
        ) -> dito_core::error::ProviderResult<Option<Arc<dyn dito_core::provider::Polisher>>> {
            self.0.behavior(|b| b.fail_status = Some(503));
            self.1.polisher(c).await
        }
    }
    let engines = Flip(&srv, f);
    let clip = tone();
    let out = pipeline::run(
        &engines,
        &cfg,
        &PipelineJob { clip: &clip, style: Style::General, wordbook: &[] },
        &|_| {},
        &CancellationToken::new(),
    )
    .await
    .map_err(|e| match e {
        PipelineError::Cancelled => "cancelled".to_string(),
        PipelineError::Failed(e) => e.to_string(),
    })
    .unwrap();
    assert_eq!(out.text(), "えーと 明日の3時に会議があります");
    assert!(out.polish_error.is_some());
}
