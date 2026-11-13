//! UI copy, compiled from [`messages.toml`](./i18n/messages.toml) by `build.rs`:
//! - Each dotted key becomes a `Tr` variant (`nav.settings` -> `Tr::NavSettings`) embedded at compile time.
//! - Every entry requires non-empty `ja`, `en`, and `zh` strings; missing languages or placeholder drift fail the build.
//! - Parameterized templates live under `fmt.` (`{name}` placeholders, literal braces doubled as `{{` / `}}`) and get
//!   generated functions under [`fmt`]; plain entries render braces verbatim and reject bare placeholders.

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
