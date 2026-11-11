//! A local mock of the cloud endpoints Dito talks to, for integration tests and `dito-mock-server`.

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
