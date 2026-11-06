use async_trait::async_trait;

use crate::error::ProviderResult;

pub struct PolishRequest<'a> {
    pub system: &'a str,
    pub text: &'a str,
}

#[async_trait]
pub trait Polisher: Send + Sync {
    async fn polish(&self, req: &PolishRequest<'_>) -> ProviderResult<String>;
}
