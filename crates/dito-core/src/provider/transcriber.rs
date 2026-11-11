use async_trait::async_trait;

use crate::audio::AudioClip;
use crate::error::ProviderResult;

pub struct AsrRequest<'a> {
    pub clip: &'a AudioClip,
    /// ISO-639-1 code; `None` detects.
    pub language: Option<&'a str>,
    /// Wordbook words, passed as hotwords where the engine supports it.
    pub hotwords: &'a [String],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    pub text: String,
    pub language: Option<String>,
}

#[async_trait]
pub trait Transcriber: Send + Sync {
    async fn transcribe(&self, req: &AsrRequest<'_>) -> ProviderResult<Transcript>;
}
