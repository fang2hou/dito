//! Speech recognition and polish engines behind two small traits.

pub mod bedrock;
pub mod configured_asr;
pub mod mock;
pub mod openai;
mod polisher;
mod test_outcome;
mod timeout;
mod transcriber;

pub use polisher::{Example, PolishRequest, Polisher};
pub use test_outcome::{TestOutcome, test_polisher, test_transcriber, test_transcriber_with_language};
pub(crate) use timeout::default_timeout;
pub use transcriber::{AsrRequest, Transcriber, Transcript};
