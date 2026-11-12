use async_trait::async_trait;

use crate::error::ProviderResult;

/// One demonstration: a user turn and the assistant reply a model should have given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Example {
    pub said: &'static str,
    pub written: &'static str,
}

pub struct PolishRequest<'a> {
    pub system: &'a str,
    /// Sent as earlier turns of the conversation, in order, before `text`.
    pub examples: &'static [Example],
    /// The last user turn.
    pub text: &'a str,
}

#[async_trait]
pub trait Polisher: Send + Sync {
    async fn polish(&self, req: &PolishRequest<'_>) -> ProviderResult<String>;
}
