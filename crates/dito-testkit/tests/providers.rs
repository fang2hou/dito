//! Provider engines against the mock server: success, auth, rate limit, outage, fallbacks.

use std::sync::Arc;
use std::time::Duration;

use dito_core::audio::AudioClip;
use dito_core::config::{Config, ProviderKind, ReasoningEffort, Source};
use dito_core::error::ErrorKind;
use dito_core::factory::{EngineFactory, Engines};
use dito_core::pipeline::CancellationToken;
use dito_core::pipeline::{self, Job, PipelineError};
use dito_core::prompts::Style;
use dito_core::provider::{AsrRequest, PolishRequest, TestOutcome, test_polisher, test_transcriber};
use dito_core::secrets::{MemorySecrets, Role, SecretStore, account};
use dito_testkit::MockServer;

fn tone() -> AudioClip {
    AudioClip::new(vec![0.3; 16_000])
}

fn azure_cfg(url: &str) -> Config {
    let mut c = Config::dev_azure();
    c.asr.provider.azure.endpoint = url.into();
    c.polish.engine.provider.azure.endpoint = url.into();
    c
}

fn factory(key: Option<&str>) -> EngineFactory {
    let secrets = Arc::new(MemorySecrets::default());
    if let Some(k) = key {
        secrets.set(&account(Role::Asr, ProviderKind::Azure), k).unwrap();
        secrets.set(&account(Role::Polish, ProviderKind::Azure), k).unwrap();
        secrets.set(&account(Role::Asr, ProviderKind::Compat), k).unwrap();
        secrets.set(&account(Role::Polish, ProviderKind::Compat), k).unwrap();
    }
    EngineFactory::new(secrets)
}

#[tokio::test]
async fn azure_asr_and_polish_hit_the_v1_surface_with_deployment_names_and_key() {
    let srv = MockServer::start().await.unwrap();
    srv.behavior(|b| b.expected_key = Some("secret".into()));
    let cfg = azure_cfg(&srv.url);
    let f = factory(Some("secret"));

    let asr = f.transcriber(&cfg).await.unwrap();
    let words = vec!["Acme".to_string()];
    let clip = tone();
    let t = asr.transcribe(&AsrRequest { clip: &clip, language: Some("ja"), hotwords: &words }).await.unwrap();
    assert_eq!(t.text, "えーと 明日の3時に会議があります");
    assert_eq!(t.language.as_deref(), Some("ja"));

    let polish = f.polisher(&cfg).await.unwrap().unwrap();
    let out = polish.polish(&PolishRequest { system: "sys", text: "えーと 会議です" }).await.unwrap();
    assert_eq!(out, "会議です。");

    let reqs = srv.requests();
    assert_eq!(reqs[0].model.as_deref(), Some("MAI-Transcribe-2"));
    assert_eq!(reqs[0].language.as_deref(), Some("ja"));
    assert_eq!(reqs[0].prompt.as_deref(), Some("Vocabulary: Acme"));
    assert_eq!(reqs[0].audio_bytes, 44 + 32_000, "WAV header + 1 s of 16-bit samples");
    assert_eq!(reqs[1].model.as_deref(), Some("gpt-6.1-terra"));
    assert_eq!(reqs[1].reasoning_effort.as_deref(), Some("low"));
    assert_eq!(reqs[1].system.as_deref(), Some("sys"));
}

#[tokio::test]
async fn wrong_key_is_classified_as_auth_for_both_engines() {
    let srv = MockServer::start().await.unwrap();
    srv.behavior(|b| b.expected_key = Some("right".into()));
    let cfg = azure_cfg(&srv.url);
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
    let cfg = azure_cfg(&srv.url);
    let f = factory(None);
    for (status, kind) in [(429, ErrorKind::RateLimit), (404, ErrorKind::ModelNotFound), (500, ErrorKind::Other)] {
        srv.behavior(|b| b.fail_status = Some(status));
        let engine = f.polisher(&cfg).await.unwrap().unwrap();
        let err = engine.polish(&PolishRequest { system: "s", text: "t" }).await.unwrap_err();
        assert_eq!(err.kind, kind, "status {status}");
    }
}

#[tokio::test]
async fn unreachable_server_is_a_network_error_not_a_hang() {
    let srv = MockServer::start().await.unwrap();
    let url = srv.url.clone();
    drop(srv);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let f = factory(None);
    let engine = f.polisher(&azure_cfg(&url)).await.unwrap().unwrap();
    let err = tokio::time::timeout(Duration::from_secs(15), engine.polish(&PolishRequest { system: "s", text: "t" }))
        .await
        .expect("must fail fast")
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Network);
}

#[tokio::test]
async fn connection_test_reports_latency_and_treats_no_speech_as_reachable() {
    let srv = MockServer::start().await.unwrap();
    srv.behavior(|b| {
        b.latency = Duration::from_millis(40);
        b.transcript = String::new(); // the engine answers with an empty transcript for silence
    });
    let f = factory(None);
    let asr = f.transcriber(&azure_cfg(&srv.url)).await.unwrap();
    match test_transcriber(asr.as_ref()).await {
        TestOutcome::Ok { latency_ms } => assert!(latency_ms >= 40, "latency {latency_ms}"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn compat_accepts_a_pasted_chat_completions_url_and_uses_bearer_auth() {
    let srv = MockServer::start().await.unwrap();
    srv.behavior(|b| b.expected_key = Some("tok".into()));
    let mut cfg = Config::default();
    cfg.polish.engine.source = Source::Provider;
    cfg.polish.engine.provider.kind = ProviderKind::Compat;
    cfg.polish.engine.provider.compat.base_url = format!("{}/v1/chat/completions", srv.url);
    cfg.polish.engine.provider.compat.model = "gpt-4o-mini".into();
    let f = factory(Some("tok"));
    let p = f.polisher(&cfg).await.unwrap().unwrap();
    assert_eq!(p.polish(&PolishRequest { system: "s", text: "um hello" }).await.unwrap(), "hello.");
    let r = &srv.requests()[0];
    assert!(r.headers.iter().any(|(k, v)| k == "authorization" && v == "Bearer tok"));
}

#[tokio::test]
async fn reasoning_effort_is_dropped_and_retried_for_models_that_reject_it() {
    let srv = MockServer::start().await.unwrap();
    srv.behavior(|b| b.reject_reasoning_effort = true);
    let mut cfg = azure_cfg(&srv.url);
    cfg.polish.engine.provider.azure.reasoning_effort = ReasoningEffort::High;
    let p = factory(None).polisher(&cfg).await.unwrap().unwrap();
    assert_eq!(p.polish(&PolishRequest { system: "s", text: "ok" }).await.unwrap(), "ok.");
    let reqs = srv.requests();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0].reasoning_effort.as_deref(), Some("high"));
    assert_eq!(reqs[1].reasoning_effort, None);
}

#[tokio::test]
async fn missing_endpoint_fails_before_any_request() {
    let cfg = Config::dev_azure(); // endpoint left empty
    let err = factory(None).transcriber(&cfg).await.err().unwrap();
    assert_eq!(err.kind, ErrorKind::Unavailable);
}

#[tokio::test]
async fn full_pipeline_over_http_with_polish_outage_still_delivers_raw_text() {
    let srv = MockServer::start().await.unwrap();
    let cfg = azure_cfg(&srv.url);
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
        &Job { clip: &clip, style: Style::General, dictionary: &[] },
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
