//! UI copy. The single source is [`messages.toml`](./messages.toml): one template ID per
//! message with every language side by side. `build.rs` compiles it into the [`Tr`] enum
//! and the `fmt::*` formatting functions, so:
//!
//! * referencing an ID that does not exist is a compile error,
//! * a missing language or an empty value fails the build,
//! * placeholder drift between languages fails the build,
//! * all copy is embedded in the binary — no file I/O at runtime.
//!
//! Adding a language = a column in the TOML + one entry in build.rs's `LANGS`.
//! Call sites: `lang.tr(Tr::NavSettings)`; parameterized copy: `i18n::fmt::using_model(lang, name)`.
//! Braces in copy: `template` has the rules.

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
