//! A local mock of the cloud endpoints Dito talks to: Azure OpenAI, OpenAI-compatible servers and
//! Bedrock Converse. Used by integration tests and by `dito-mock-server` for offline development.

mod bedrock;
mod behavior;
mod files;
mod mock_server;
mod openai;
mod recorded;
mod shared;

pub use behavior::Behavior;
pub use mock_server::MockServer;
pub use recorded::Recorded;
