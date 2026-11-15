use std::time::Instant;

use serde::{Deserialize, Serialize};

use super::{AsrRequest, TextModel, TextRequest, Transcriber};
use crate::audio::AudioClip;
use crate::error::{ErrorKind, ProviderError, ProviderResult};

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

/// An engine that answers "no speech" is reachable, so that counts as success.
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

pub async fn test_text_model(p: &dyn TextModel) -> TestOutcome {
    let started = Instant::now();
    let r = p.polish(&TextRequest { system: "Reply with the user's text unchanged.", examples: &[], text: "ok" }).await;
    TestOutcome::from_result(started, r.map(|_| ()))
}
