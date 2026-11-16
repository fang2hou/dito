//! Speech recognition and polish engines behind two small traits.

pub mod bedrock;
pub mod configured_asr;
pub mod mock;
pub mod openai;
mod router;
mod test_outcome;
mod text_model;
mod timeout;
mod transcriber;

pub use router::Router;
pub use test_outcome::{TestOutcome, test_text_model, test_transcriber, test_transcriber_with_language};
pub use text_model::{ChatCancel, ChatDelta, ChatMessage, ChatRequest, ChatRole, Example, TextModel, TextRequest};
pub(crate) use timeout::default_timeout;
pub use transcriber::{AsrRequest, Transcriber, Transcript};
