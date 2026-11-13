//! The debug log of what a run does: the key actions and decisions of a dictation or a command, with the full prompts
//! and texts, for the testing phase.
//!
//! Every line goes to one target, [`TARGET`], at debug level, so it is off by default and can be switched on without
//! any other debug output: `RUST_LOG=dito_trace=debug`, or the Detailed log switch of the app's Status page. A run
//! opens a span named `run` that carries its id, so every line of that run, from any crate, shows `run=<id>` and a text
//! search follows it. Keys and audio are never logged.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::Span;

use crate::intent::Decision;

#[doc(hidden)]
pub use tracing as __tracing;

pub const TARGET: &str = "dito_trace";

/// One debug line under [`TARGET`], with the arguments of `tracing::debug!`. Texts go in with `?`, which keeps a line
/// to one line.
#[macro_export]
macro_rules! trace_log {
    ($($arg:tt)+) => {
        $crate::trace::__tracing::debug!(target: $crate::trace::TARGET, $($arg)+)
    };
}

/// Names one run in the log: `d` for a dictation, `c` for a command, then the start time in milliseconds as six hex
/// digits, which repeats only after four and a half hours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunId(String);

impl RunId {
    pub fn new(kind: char) -> Self {
        let ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        Self(format!("{kind}{:06x}", ms & 0xff_ffff))
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The span every line of the run is logged inside. Disabled together with the target. It is a root span: a run that
/// starts while the handler of the previous one is still entered must not show that run's id as well.
pub fn run_span(id: &RunId) -> Span {
    tracing::debug_span!(target: TARGET, parent: None, "run", run = %id)
}

const PREVIEW_CHARS: usize = 40;

/// The start of `text` in quotes, for a line that needs only to say which text it is about.
pub fn preview(text: &str) -> String {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(PREVIEW_CHARS).collect();
    let cut = chars.next().is_some();
    format!("{head:?}{}", if cut { "…" } else { "" })
}

/// Every intent with its probability, most likely first, for example `polish=0.912 rewrite=0.050`.
pub fn ranked_line(decision: &Decision) -> String {
    let ranked = decision.ranked();
    ranked.iter().map(|(intent, p)| format!("{}={p:.3}", intent.label())).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::Intent;
    use crate::provider::mock::decision;

    #[test]
    fn a_preview_is_cut_on_a_character_boundary_and_marked() {
        assert_eq!(preview("短い"), "\"短い\"");
        let long = "あ".repeat(PREVIEW_CHARS + 1);
        assert_eq!(preview(&long), format!("\"{}\"…", "あ".repeat(PREVIEW_CHARS)));
    }

    #[test]
    fn the_ranked_line_lists_every_intent_most_likely_first() {
        let line = ranked_line(&decision(Intent::Translate, 0.9));
        assert!(line.starts_with("translate=0.900 polish=0.014"), "{line}");
        assert_eq!(line.split(' ').count(), Intent::COUNT);
    }
}
