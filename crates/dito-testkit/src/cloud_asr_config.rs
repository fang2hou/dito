use serde_json::{Value, json};

/// A cloud speech request JSON in the ADR-0007 format for the mock server at `server_url`.
///
/// The request is a multipart POST to `<server_url>/v1/audio/transcriptions` with the fields the mock reads: `file`
/// (`{{audio.file}}`), `model` (`{{model}}`) and `language` (`{{language}}`). The key goes in the header
/// `Authorization: Bearer {{api_key}}`, the model is `mock-asr`, and the transcript and language are read from
/// `/text` and `/language` of the response. The JSON passes `configured_asr::validate`.
pub fn mock_cloud_asr_config(server_url: &str) -> Value {
    json!({
        "version": 1,
        "connection": { "base_url": server_url, "model": "mock-asr" },
        "language_map": { "ja": "ja", "en": "en", "zh": "zh", "auto": null, "other": "en" },
        "request": {
            "method": "POST",
            "path": "/v1/audio/transcriptions",
            "query": {},
            "headers": { "Authorization": "Bearer {{api_key}}" },
            "encoding": "multipart",
            "fields": { "file": "{{audio.file}}", "model": "{{model}}", "language": "{{language}}" }
        },
        "response": { "text_pointer": "/text", "language_pointer": "/language" }
    })
}

#[cfg(test)]
mod tests {
    use dito_core::audio::AudioClip;
    use dito_core::provider::configured_asr::{ConfiguredAsr, validate};
    use dito_core::provider::{AsrRequest, Transcriber};

    use super::mock_cloud_asr_config;
    use crate::MockServer;

    #[tokio::test]
    async fn the_request_json_validates_and_a_configured_engine_transcribes_against_the_mock() {
        let srv = MockServer::start().await.unwrap();
        let config = mock_cloud_asr_config(&srv.url);
        validate(&config).unwrap();

        let engine = ConfiguredAsr::new(config, "key".into()).unwrap();
        let clip = AudioClip::new(vec![0.3; 16_000]);
        let t = engine.transcribe(&AsrRequest { clip: &clip, language: Some("ja"), hotwords: &[] }).await.unwrap();
        assert_eq!(t.text, "えーと 明日の3時に会議があります");
        assert_eq!(t.language.as_deref(), Some("ja"));

        let reqs = srv.requests();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].model.as_deref(), Some("mock-asr"));
        assert_eq!(reqs[0].language.as_deref(), Some("ja"));
        assert!(reqs[0].audio_bytes > 0, "the audio part reaches the mock");
    }
}
