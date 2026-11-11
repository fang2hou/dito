use axum::{
    Json, Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, Uri},
    routing::post,
};
use dito_core::{
    audio::AudioClip,
    provider::{
        AsrRequest, Transcriber,
        configured_asr::{CloudAsrConfigurations, ConfiguredAsr},
    },
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
type RecordedRequest = (String, HeaderMap, Vec<u8>);
#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<RecordedRequest>>>);
async fn endpoint(State(c): State<Capture>, uri: Uri, headers: HeaderMap, body: Bytes) -> Json<Value> {
    c.0.lock().unwrap().push((uri.to_string(), headers, body.to_vec()));
    Json(
        json!({"text":"transcribed","output":{"text":"transcribed"},"choices":[{"message":{"content":"transcribed"}}]}),
    )
}
async fn server() -> (String, Capture, tokio::task::JoinHandle<()>) {
    let c = Capture(Arc::new(Mutex::new(Vec::new())));
    let app = Router::new().fallback(post(endpoint)).with_state(c.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{address}/relay/v1"), c, task)
}
#[tokio::test]
async fn all_templates_send_their_own_format_preserve_relay_prefix_and_extract_text() {
    let (url, c, task) = server().await;
    let clip = AudioClip::silence(0.1);
    for (index, configuration) in CloudAsrConfigurations::default().items.into_iter().enumerate() {
        let mut v = configuration.config;
        v["connection"]["base_url"] = url.clone().into();
        v["connection"]["model"] = "future-model".into();
        v["language_map"]["ja"] = "JP".into();
        let engine = ConfiguredAsr::new(v, "secret".into()).unwrap();
        let result =
            engine.transcribe(&AsrRequest { clip: &clip, language: Some("ja-JP"), hotwords: &[] }).await.unwrap();
        assert_eq!(result.text, "transcribed");
        let requests = c.0.lock().unwrap();
        let (path, headers, raw) = &requests[index];
        assert!(path.starts_with("/relay/v1/"));
        assert_eq!(headers["authorization"], "Bearer secret");
        let body: Value = serde_json::from_slice(raw).unwrap();
        assert_eq!(body["model"], "future-model");
        match index {
            0 => assert!(body["input"]["messages"].is_array()),
            1 => assert_eq!(body["asr_options"]["language"], "JP"),
            2 => assert_eq!(body["language"], "JP"),
            _ => unreachable!(),
        }
    }
    task.abort();
}
#[tokio::test]
async fn auto_language_is_omitted_and_unknown_language_uses_other() {
    let (url, c, task) = server().await;
    let mut v = CloudAsrConfigurations::default().items[2].config.clone();
    v["connection"]["base_url"] = url.into();
    v["connection"]["model"] = "my-model".into();
    let engine = ConfiguredAsr::new(v, String::new()).unwrap();
    let clip = AudioClip::silence(0.1);
    for language in [None, Some("fr")] {
        engine.transcribe(&AsrRequest { clip: &clip, language, hotwords: &[] }).await.unwrap();
    }
    let r = c.0.lock().unwrap();
    let a: Value = serde_json::from_slice(&r[0].2).unwrap();
    let b: Value = serde_json::from_slice(&r[1].2).unwrap();
    assert!(a.get("language").is_none());
    assert_eq!(b["language"], "en");
    task.abort();
}
#[tokio::test]
async fn custom_multipart_sends_binary_audio_and_encoded_query() {
    let (url, c, task) = server().await;
    let mut v = CloudAsrConfigurations::default().items[2].config.clone();
    v["connection"]["base_url"] = url.into();
    v["connection"]["model"] = "my-model".into();
    v["request"] = json!({"method":"POST","path":"/transcribe","headers":{"X-API-Key":"{{api_key}}"},"query":{"name":"日本語 & test"},"encoding":"multipart","fields":{"file":"{{audio.file}}","language":"{{language}}","model":"{{model}}"}});
    let clip = AudioClip::silence(0.1);
    ConfiguredAsr::new(v, "secret".into())
        .unwrap()
        .transcribe(&AsrRequest { clip: &clip, language: None, hotwords: &[] })
        .await
        .unwrap();
    let r = c.0.lock().unwrap();
    assert!(r[0].0.contains("name=%E6%97%A5"));
    assert_eq!(r[0].1["x-api-key"], "secret");
    assert!(r[0].1["content-type"].to_str().unwrap().contains("boundary="));
    let raw = String::from_utf8_lossy(&r[0].2);
    assert!(raw.contains("audio.wav"));
    assert!(!raw.contains("name=\"language\""));
    assert!(r[0].2.windows(4).any(|x| x == b"RIFF"));
    task.abort();
}
