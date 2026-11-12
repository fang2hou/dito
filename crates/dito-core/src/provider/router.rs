use async_trait::async_trait;

use crate::error::ProviderResult;
use crate::intent::Decision;

/// Decides what a spoken instruction asks for. The implementation runs the loaded chat model; the decision layer is
/// [`crate::intent`].
#[async_trait]
pub trait Router: Send + Sync {
    async fn decide(&self, instruction: &str) -> ProviderResult<Decision>;
}
