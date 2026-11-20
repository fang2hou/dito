//! The service end to end: a real listener on a free loopback port, driven with `reqwest`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};

use super::*;
use crate::audio::AudioClip;
use crate::provider::mock::{MockTextModel, MockTranscriber};
use crate::provider::{AsrRequest, ChatCancel, ChatDelta, ChatRequest, Transcript};

struct Fake {
    text: Arc<dyn TextModel>,
    speech: Arc<dyn Transcriber>,
}

#[async_trait]
impl Backend for Fake {
    async fn text_model(&self, _: &Config) -> ProviderResult<Arc<dyn TextModel>> {
        Ok(self.text.clone())
    }

    async fn transcriber(&self, _: &Config) -> ProviderResult<Arc<dyn Transcriber>> {
        Ok(self.speech.clone())
    }
}

struct NoModels;
impl InstalledModels for NoModels {
    fn installed(&self) -> Vec<InstalledModel> {
        vec![InstalledModel { id: "spare-model".into(), name: "Spare".into(), role: ModelRole::Text }]
    }
}

/// Records the request it receives and streams `pieces` one by one.
struct Streaming {
    seen: Mutex<Option<ChatRequest>>,
    pieces: Vec<&'static str>,
}

#[async_trait]
impl TextModel for Streaming {
    async fn polish(&self, _: &crate::provider::TextRequest<'_>) -> ProviderResult<String> {
        unreachable!("the service talks to the chat contract")
    }

    async fn chat(&self, req: &ChatRequest, on_delta: ChatDelta<'_>, _: &ChatCancel) -> ProviderResult<String> {
        *self.seen.lock().unwrap() = Some(req.clone());
        for piece in &self.pieces {
            on_delta(piece);
        }
        Ok(self.pieces.concat())
    }
}

/// Generates until it is cancelled and reports that it saw the cancellation.
struct Endless {
    cancelled: Arc<AtomicBool>,
}

#[async_trait]
impl TextModel for Endless {
    async fn polish(&self, _: &crate::provider::TextRequest<'_>) -> ProviderResult<String> {
        unreachable!()
    }

    async fn chat(&self, _: &ChatRequest, on_delta: ChatDelta<'_>, cancel: &ChatCancel) -> ProviderResult<String> {
        while !cancel.is_cancelled() {
            on_delta("tick ");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        self.cancelled.store(true, Ordering::SeqCst);
        Ok(String::new())
    }
}

struct Fixture {
    api: LocalApi,
    cfg: Arc<ConfigStore>,
    port: u16,
    client: reqwest::Client,
    token: String,
    _dir: tempfile::TempDir,
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

async fn running(api: &LocalApi, port: u16) {
    let mut status = api.status();
    tokio::time::timeout(Duration::from_secs(5), status.wait_for(|s| *s == Status::Running { port }))
        .await
        .expect("the service starts")
        .unwrap();
}

async fn fixture(text: Arc<dyn TextModel>, speech: Arc<dyn Transcriber>) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let cfg = Arc::new(ConfigStore::open(dir.path().join("config.toml")).unwrap());
    let port = free_port();
    cfg.update(|c| {
        c.local_api.enabled = true;
        c.local_api.port = port;
    })
    .unwrap();
    let api = LocalApi::spawn(
        Deps {
            cfg: cfg.clone(),
            token_path: dir.path().join("local_api.token"),
            backend: Arc::new(Fake { text, speech }),
            installed: Arc::new(NoModels),
        },
        &tokio::runtime::Handle::current(),
    );
    running(&api, port).await;
    let token = api.token();
    Fixture { api, cfg, port, client: reqwest::Client::builder().no_proxy().build().unwrap(), token, _dir: dir }
}

async fn plain() -> Fixture {
    fixture(Arc::new(MockTextModel::default()), Arc::new(MockTranscriber::new("こんにちは"))).await
}

impl Fixture {
    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.client.get(self.url(path)).bearer_auth(&self.token)
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.client.post(self.url(path)).bearer_auth(&self.token)
    }
}

#[tokio::test]
async fn a_request_from_a_browser_or_a_foreign_host_is_refused_even_with_the_token() {
    let f = plain().await;
    let rebound = f.get("/v1/models").header("Host", "attacker.example").send().await.unwrap();
    assert_eq!(rebound.status(), 403);
    let wrong_port = f.get("/v1/models").header("Host", "127.0.0.1:80").send().await.unwrap();
    assert_eq!(wrong_port.status(), 403);
    let browser = f.get("/v1/models").header("Origin", "https://attacker.example").send().await.unwrap();
    assert_eq!(browser.status(), 403);
    let mcp = f.post("/mcp").header("Origin", "https://attacker.example").send().await.unwrap();
    assert_eq!(mcp.status(), 403, "the guard covers MCP too");
    assert_eq!(f.get("/v1/models").send().await.unwrap().status(), 200);
}

#[tokio::test]
async fn the_api_wants_the_token_until_any_token_is_switched_on() {
    let f = plain().await;
    let bare = f.client.get(f.url("/v1/models")).send().await.unwrap();
    assert_eq!(bare.status(), 401);
    assert_eq!(bare.headers()["www-authenticate"], "Bearer");
    assert_eq!(bare.json::<Value>().await.unwrap()["error"]["code"], "invalid_api_key");
    let wrong = f.client.get(f.url("/v1/models")).bearer_auth("nope").send().await.unwrap();
    assert_eq!(wrong.status(), 401);
    assert_eq!(f.get("/v1/models").send().await.unwrap().status(), 200);

    f.cfg.update(|c| c.local_api.api_any_token = true).unwrap();
    assert_eq!(f.client.get(f.url("/v1/models")).send().await.unwrap().status(), 200);
}

#[tokio::test]
async fn a_replaced_token_takes_effect_at_once() {
    let f = plain().await;
    f.api.set_token("my-own-token").unwrap();
    assert_eq!(f.get("/v1/models").send().await.unwrap().status(), 401, "the old token is gone");
    let renewed = f.client.get(f.url("/v1/models")).bearer_auth("my-own-token").send().await.unwrap();
    assert_eq!(renewed.status(), 200);
}

#[tokio::test]
async fn models_name_the_text_and_the_speech_model_in_use() {
    let f = plain().await;
    let body: Value = f.get("/v1/models").send().await.unwrap().json().await.unwrap();
    let cfg = f.cfg.get();
    assert_eq!(body["object"], "list");
    assert_eq!(body["data"][0]["id"], cfg.text_model.local_model.as_str());
    assert_eq!(body["data"][0]["role"], "text");
    assert_eq!(body["data"][1]["id"], cfg.asr.local_model.as_str());
}

#[tokio::test]
async fn a_chat_completion_reaches_the_model_as_a_conversation_and_returns_its_reply() {
    let model = Arc::new(Streaming { seen: Mutex::default(), pieces: vec!["Hel", "lo"] });
    let f = fixture(model.clone(), Arc::new(MockTranscriber::new(""))).await;
    let reply: Value = f
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "whatever",
            "messages": [{ "role": "system", "content": "Be brief." }, { "role": "user", "content": "hi" }],
            "temperature": 0.5,
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reply["object"], "chat.completion");
    assert_eq!(reply["choices"][0]["message"]["content"], "Hello");
    let seen = model.seen.lock().unwrap().clone().unwrap();
    assert_eq!(seen.messages.len(), 2);
    assert_eq!(seen.temperature, Some(0.5));
}

#[tokio::test]
async fn a_streamed_completion_sends_chunks_in_order_and_ends_with_done() {
    let model = Arc::new(Streaming { seen: Mutex::default(), pieces: vec!["Hel", "lo"] });
    let f = fixture(model, Arc::new(MockTranscriber::new(""))).await;
    let response = f
        .post("/v1/chat/completions")
        .json(&json!({ "messages": [{ "role": "user", "content": "hi" }], "stream": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    let body = response.text().await.unwrap();
    let events: Vec<&str> = body.lines().filter_map(|l| l.strip_prefix("data:")).map(str::trim).collect();
    assert_eq!(events.last(), Some(&"[DONE]"));
    let texts: Vec<String> = events
        .iter()
        .filter_map(|e| serde_json::from_str::<Value>(e).ok())
        .filter_map(|v| v["choices"][0]["delta"]["content"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(texts, ["", "Hel", "lo"], "the role chunk opens, then each piece");
    assert!(events[events.len() - 2].contains(r#""finish_reason":"stop""#));
}

#[tokio::test]
async fn closing_the_connection_cancels_a_streaming_reply() {
    let cancelled = Arc::new(AtomicBool::new(false));
    let f = fixture(Arc::new(Endless { cancelled: cancelled.clone() }), Arc::new(MockTranscriber::new(""))).await;
    let mut response = f
        .post("/v1/chat/completions")
        .json(&json!({ "messages": [{ "role": "user", "content": "go" }], "stream": true }))
        .send()
        .await
        .unwrap();
    assert!(response.chunk().await.unwrap().is_some(), "the stream is live");
    drop(response);
    tokio::time::timeout(Duration::from_secs(5), async {
        while !cancelled.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the model saw the cancellation");
}

#[tokio::test]
async fn a_failed_model_answers_with_its_kind_and_no_provider_detail() {
    let failing = MockTextModel::default().script([Err(crate::ErrorKind::Auth)]);
    let f = fixture(Arc::new(failing), Arc::new(MockTranscriber::new(""))).await;
    let response = f
        .post("/v1/chat/completions")
        .json(&json!({ "messages": [{ "role": "user", "content": "hi" }] }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 502);
    let body = response.text().await.unwrap();
    assert!(body.contains("upstream_auth") && !body.contains("scripted failure"), "{body}");
    let malformed = f.post("/v1/chat/completions").body("not json").send().await.unwrap();
    assert_eq!(malformed.status(), 400);
}

fn wav_form(bytes: Vec<u8>, extra: &[(&'static str, &'static str)]) -> reqwest::multipart::Form {
    let file = reqwest::multipart::Part::bytes(bytes).file_name("speech.wav");
    extra.iter().fold(reqwest::multipart::Form::new().part("file", file), |form, (k, v)| form.text(*k, *v))
}

#[tokio::test]
async fn a_wav_upload_is_transcribed_by_the_speech_model() {
    let f = plain().await;
    let wav = AudioClip::silence(0.5).to_wav();
    let json: Value = f
        .post("/v1/audio/transcriptions")
        .multipart(wav_form(wav.clone(), &[("model", "whisper-1")]))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(json, json!({ "text": "こんにちは" }));
    let text = f
        .post("/v1/audio/transcriptions")
        .multipart(wav_form(wav, &[("response_format", "text")]))
        .send()
        .await
        .unwrap();
    assert_eq!(text.text().await.unwrap(), "こんにちは");
}

#[tokio::test]
async fn uploads_that_are_not_wav_or_lack_a_file_are_refused() {
    let f = plain().await;
    let mp3 = f
        .post("/v1/audio/transcriptions")
        .multipart(wav_form(b"ID3 not audio we can read".to_vec(), &[]))
        .send()
        .await
        .unwrap();
    assert_eq!(mp3.status(), 415);
    let empty = f
        .post("/v1/audio/transcriptions")
        .multipart(reqwest::multipart::Form::new().text("model", "x"))
        .send()
        .await
        .unwrap();
    assert_eq!(empty.status(), 400);
}

struct Echo;

#[async_trait]
impl Transcriber for Echo {
    async fn transcribe(&self, req: &AsrRequest<'_>) -> ProviderResult<Transcript> {
        Ok(Transcript { text: format!("{}:{:?}", req.clip.samples.len(), req.language), language: None })
    }
}

#[tokio::test]
async fn the_clip_is_16_khz_mono_and_the_language_is_passed_on() {
    let f = fixture(Arc::new(MockTextModel::default()), Arc::new(Echo)).await;
    let wav = AudioClip::silence(1.0).to_wav();
    let text: Value = f
        .post("/v1/audio/transcriptions")
        .multipart(wav_form(wav, &[("language", "ja")]))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(text["text"], "16000:Some(\"ja\")");
}

#[tokio::test]
async fn a_new_port_restarts_the_service_and_turning_it_off_closes_the_port() {
    let f = plain().await;
    let old = f.port;
    let next = free_port();
    f.cfg.update(|c| c.local_api.port = next).unwrap();
    running(&f.api, next).await;
    let moved = Fixture { port: next, ..f };
    assert_eq!(moved.get("/v1/models").send().await.unwrap().status(), 200);
    assert!(moved.client.get(format!("http://127.0.0.1:{old}/v1/models")).send().await.is_err());

    moved.cfg.update(|c| c.local_api.enabled = false).unwrap();
    let mut status = moved.api.status();
    status.wait_for(|s| *s == Status::Stopped).await.unwrap();
    assert!(moved.get("/v1/models").send().await.is_err());
}

#[tokio::test]
async fn a_port_another_app_holds_is_reported_not_fatal() {
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = held.local_addr().unwrap().port();
    let f = plain().await;
    f.cfg.update(|c| c.local_api.port = port).unwrap();
    let mut status = f.api.status();
    let failed = status.wait_for(|s| matches!(s, Status::Failed { .. })).await.unwrap().clone();
    assert!(matches!(failed, Status::Failed { port: p, .. } if p == port));
    drop(held);
    let free = free_port();
    f.cfg.update(|c| c.local_api.port = free).unwrap();
    running(&f.api, free).await;
}

fn mcp_call(f: &Fixture, session: Option<&str>, body: Value) -> reqwest::RequestBuilder {
    let request = f
        .post("/mcp")
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json")
        .header("MCP-Protocol-Version", "2025-03-26")
        .body(body.to_string());
    match session {
        Some(id) => request.header("Mcp-Session-Id", id),
        None => request,
    }
}

#[tokio::test]
async fn mcp_lists_its_tools_and_resource_and_runs_them() {
    let model = Arc::new(Streaming { seen: Mutex::default(), pieces: vec!["generated"] });
    let f = fixture(model, Arc::new(MockTranscriber::new(""))).await;
    let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } } });
    let response = mcp_call(&f, None, init).send().await.unwrap();
    assert_eq!(response.status(), 200);
    let session = response.headers()["mcp-session-id"].to_str().unwrap().to_owned();
    assert!(response.text().await.unwrap().contains("serverInfo"));
    let ready = mcp_call(&f, Some(&session), json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    assert_eq!(ready.send().await.unwrap().status(), 202);

    let call = |id: u32, method: &str, params: Value| {
        mcp_call(&f, Some(&session), json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
    };
    let tools = call(2, "tools/list", json!({})).send().await.unwrap().text().await.unwrap();
    assert!(tools.contains("generate_text") && tools.contains("list_local_models"), "{tools}");
    let resources = call(3, "resources/list", json!({})).send().await.unwrap().text().await.unwrap();
    assert!(resources.contains("dito://models"), "{resources}");
    let generated = call(4, "tools/call", json!({ "name": "generate_text", "arguments": { "prompt": "hi" } }))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(generated.contains("generated"), "{generated}");
    let listed = call(5, "tools/call", json!({ "name": "list_local_models", "arguments": {} }))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(listed.contains("spare-model"), "{listed}");
    let read = call(6, "resources/read", json!({ "uri": "dito://models" })).send().await.unwrap().text().await.unwrap();
    assert!(read.contains("spare-model"), "{read}");
}

#[tokio::test]
async fn mcp_accepts_any_token_by_default_and_wants_the_token_when_that_is_switched_off() {
    let f = plain().await;
    let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } } });
    let anonymous = || {
        f.client
            .post(f.url("/mcp"))
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .body(init.to_string())
    };
    assert_eq!(anonymous().send().await.unwrap().status(), 200);
    f.cfg.update(|c| c.local_api.mcp_any_token = false).unwrap();
    assert_eq!(anonymous().send().await.unwrap().status(), 401);
    assert_eq!(anonymous().bearer_auth(&f.token).send().await.unwrap().status(), 200);
}

#[tokio::test]
async fn the_connection_check_reports_what_a_client_would_meet() {
    let f = plain().await;
    assert_eq!(check_connection(f.port, &f.token, Surface::Api).await, Ok(()));
    assert_eq!(check_connection(f.port, &f.token, Surface::Mcp).await, Ok(()));
    assert_eq!(check_connection(f.port, "wrong", Surface::Api).await, Err(CheckFailure::Unauthorized));
    assert_eq!(check_connection(free_port(), &f.token, Surface::Api).await, Err(CheckFailure::Unreachable));
}
