//! UI copy, compiled from [`messages.toml`](./messages.toml) by `build.rs`: an unknown ID is a compile error, a missing
//! language, an empty value or placeholder drift fails the build, and all copy is embedded, so there is no file I/O at
//! runtime.

mod generated;
mod lang;
mod sample_utterance;
mod system_language;
// The build script validates messages with this module; here it is only tested.
#[cfg(test)]
mod template;

pub use generated::{Tr, fmt};
pub use lang::Lang;
pub use sample_utterance::sample_utterance;
pub use system_language::{set_system_language, system_language};
