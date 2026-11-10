//! A local mock of the cloud endpoints Dito talks to: Azure OpenAI, OpenAI-compatible servers and
//! Bedrock Converse. Used by integration tests and by `dito-mock-server` for offline development.
//! [`mock_cloud_asr_config`] is the cloud speech request JSON that points at its transcription route.

mod bedrock;
mod behavior;
mod cloud_asr_config;
mod files;
mod mock_server;
mod openai;
mod recorded;
mod shared;

pub use behavior::Behavior;
pub use cloud_asr_config::mock_cloud_asr_config;
pub use mock_server::MockServer;
pub use recorded::Recorded;
