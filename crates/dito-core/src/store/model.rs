//! The records the store reads and writes.

use crate::config::EngineLabels;
use crate::intent::{Decision, Intent};
use crate::prompts::Style;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The database was written by a newer build. It is refused untouched: this build cannot know what changed.
    #[error("the database has schema version {found}, but this build supports up to version {supported}")]
    SchemaTooNew { found: u32, supported: u32 },
}

pub(super) type Result<T> = std::result::Result<T, StoreError>;

/// Typing speed used to estimate "time saved" (characters per minute).
pub const TYPING_CPM: f64 = 60.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HistoryKind {
    #[default]
    Dictation,
    /// The audio is kept so it can be transcribed again.
    Cancelled,
    Failed,
    /// One command run. `raw` is its instruction, `polished` its result, `source` the text it worked on, and
    /// `command` the rest of what happened; there is no audio.
    Command,
}

/// How long each step took, in milliseconds. `None`: the step did not run, or the entry predates the figures. For a
/// command, `polish` is the tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StepMillis {
    pub asr: Option<u64>,
    pub polish: Option<u64>,
}

/// How the instruction of a command reached it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionOrigin {
    Spoken,
    Typed,
}

/// Why the tool of a command ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    /// The decision passed the confidence gate.
    Confident,
    /// The decision did not pass it and the user picked in the choice window.
    UserChoice,
    /// The polish button skipped routing.
    PolishButton,
}

/// Where the text a tool worked on came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Selection,
    /// Typed into the command's text window because nothing was selected.
    Entered,
}

/// How a command run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOutcome {
    Replaced,
    Inserted,
    Copied,
    /// The answer window was shown and closed.
    Answered,
    /// A window was closed without using its result.
    Closed,
    Cancelled,
    Failed,
}

/// What a command row keeps besides the shared columns. It is stored as JSON in one column and is a persisted shape:
/// changing it needs a migration of the stored JSON (ADR-0015).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandDetails {
    /// `None` when the polish button skipped the instruction.
    pub origin: Option<InstructionOrigin>,
    /// What the router decided; `None` when it was skipped or failed.
    pub decision: Option<Decision>,
    /// `None` when no tool was chosen.
    pub choice: Option<ToolChoice>,
    /// The tool that ran, named by its intent.
    pub tool: Option<Intent>,
    pub source: Option<SourceKind>,
    pub outcome: CommandOutcome,
    pub route_ms: Option<u64>,
    /// The local chat model that routed, labelled like `EngineLabels`.
    pub router_engine: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewHistory {
    pub ts: i64,
    pub kind: HistoryKind,
    pub style: Style,
    pub app: String,
    pub lang: String,
    pub duration_ms: u64,
    pub raw: String,
    pub polished: Option<String>,
    pub error: Option<String>,
    pub audio_path: Option<String>,
    /// The engines that produced the entry; both labels are `None` for an entry that never reached an engine.
    pub engines: EngineLabels,
    pub timings: StepMillis,
    /// The text a command worked on.
    pub source: Option<String>,
    pub command: Option<CommandDetails>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub id: i64,
    pub ts: i64,
    pub kind: HistoryKind,
    pub style: Style,
    pub app: String,
    pub lang: String,
    pub duration_ms: u64,
    pub raw: String,
    pub polished: Option<String>,
    pub error: Option<String>,
    pub audio_path: Option<String>,
    /// The engines that produced the entry; both labels are `None` for an entry recorded before they were kept.
    pub engines: EngineLabels,
    pub timings: StepMillis,
    pub source: Option<String>,
    /// Set for a command row whose details this build can read.
    pub command: Option<CommandDetails>,
}

impl HistoryEntry {
    pub fn final_text(&self) -> &str {
        self.polished.as_deref().unwrap_or(&self.raw)
    }
}

#[derive(Debug, Clone, Default)]
pub struct HistoryFilter {
    pub query: Option<String>,
    pub since_ts: Option<i64>,
    pub style: Option<Style>,
    /// Display name and bundle id of every app currently configured under Prompts. A recording is that app when its
    /// stored style is the app or when the app name it recorded is one of these names; everything else outside the
    /// built-in styles is Other. Removing an app therefore sends its past entries to Other, and adding it back
    /// restores them without rewriting history.
    pub configured_apps: Vec<(String, String)>,
    pub disabled_builtin_styles: Vec<Style>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stats {
    pub chars: u64,
    pub recordings: u64,
    pub speech_ms: u64,
    pub avg_cpm: f64,
    pub saved_minutes: f64,
    /// (app name, recordings), descending.
    pub apps: Vec<(String, u64)>,
    /// (language code, recordings), descending.
    pub langs: Vec<(String, u64)>,
}
