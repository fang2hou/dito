//! Speech recognition and polish engines behind two small traits.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::audio::AudioClip;
use crate::error::{ErrorKind, ProviderError, ProviderResult};

pub mod bedrock;
pub mod configured_asr;
pub mod mock;
pub mod openai;

pub struct AsrRequest<'a> {
    pub clip: &'a AudioClip,
    /// ISO-639-1 hint (`ja`, `en`, `zh`); `None` = detect.
    pub language: Option<&'a str>,
    /// Dictionary words, passed as a vocabulary hint where the engine supports it.
    pub hotwords: &'a [String],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    pub text: String,
    pub language: Option<String>,
}

pub struct PolishRequest<'a> {
    pub system: &'a str,
    pub text: &'a str,
}

#[async_trait]
pub trait Transcriber: Send + Sync {
    async fn transcribe(&self, req: &AsrRequest<'_>) -> ProviderResult<Transcript>;
}

#[async_trait]
pub trait Polisher: Send + Sync {
    async fn polish(&self, req: &PolishRequest<'_>) -> ProviderResult<String>;
}

/// Result of the one-button connection test shown in settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TestOutcome {
    Ok { latency_ms: u64 },
    Failed { kind: ErrorKind, message: String },
}

impl TestOutcome {
    fn from_result(started: Instant, r: ProviderResult<()>) -> Self {
        match r {
            Ok(()) => Self::Ok { latency_ms: started.elapsed().as_millis() as u64 },
            Err(e) => Self::Failed { kind: e.kind, message: e.message },
        }
    }
}

/// Sends half a second of silence. An engine that answers "no speech" is reachable, so that counts as success.
pub async fn test_transcriber(t: &dyn Transcriber) -> TestOutcome {
    test_transcriber_with_language(t, None).await
}

pub async fn test_transcriber_with_language(t: &dyn Transcriber, language: Option<&str>) -> TestOutcome {
    let started = Instant::now();
    let clip = AudioClip::silence(0.5);
    let r = t.transcribe(&AsrRequest { clip: &clip, language, hotwords: &[] }).await;
    let r = match r {
        Ok(_) => Ok(()),
        Err(ProviderError { kind: ErrorKind::NoSpeech, .. }) => Ok(()),
        Err(e) => Err(e),
    };
    TestOutcome::from_result(started, r)
}

pub async fn test_polisher(p: &dyn Polisher) -> TestOutcome {
    let started = Instant::now();
    let r = p.polish(&PolishRequest { system: "Reply with the user's text unchanged.", text: "ok" }).await;
    TestOutcome::from_result(started, r.map(|_| ()))
}

pub(crate) fn default_timeout() -> Duration {
    Duration::from_secs(30)
}
