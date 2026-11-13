//! The records the store reads and writes.

use crate::config::EngineLabels;
use crate::intent::{Decision, Intent};
use crate::prompts::AppTarget;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionOrigin {
    Spoken,
    Typed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    Confident,
    /// Chosen in the HUD choice window after a low-confidence decision.
    UserChoice,
    /// Triggered directly from the command capsule's polish button without routing.
    PolishButton,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Selection,
    /// Typed into the command's text window because nothing was selected.
    Entered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOutcome {
    Replaced,
    Inserted,
    Copied,
    Answered,
    /// Dismissed without applying or copying the result.
    Closed,
    Cancelled,
    Failed,
}

/// Stored as JSON in the `command` column; persisted shape, so incompatible changes require a migration (ADR-0015).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandDetails {
    /// `None` when the polish button skipped routing.
    pub origin: Option<InstructionOrigin>,
    pub decision: Option<Decision>,
    pub choice: Option<ToolChoice>,
    pub tool: Option<Intent>,
    pub source: Option<SourceKind>,
    pub outcome: CommandOutcome,
    pub route_ms: Option<u64>,
    pub router_engine: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewHistory {
    pub ts: i64,
    pub kind: HistoryKind,
    pub style: AppTarget,
    pub app: String,
    pub lang: String,
    pub duration_ms: u64,
    pub raw: String,
    pub polished: Option<String>,
    pub error: Option<String>,
    pub audio_path: Option<String>,
    /// Both labels are `None` when a run ended before reaching an engine.
    pub engines: EngineLabels,
    pub timings: StepMillis,
    pub source: Option<String>,
    pub command: Option<CommandDetails>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub id: i64,
    pub ts: i64,
    pub kind: HistoryKind,
    pub style: AppTarget,
    pub app: String,
    pub lang: String,
    pub duration_ms: u64,
    pub raw: String,
    pub polished: Option<String>,
    pub error: Option<String>,
    pub audio_path: Option<String>,
    /// Both labels are `None` for entries recorded before engine tracking shipped.
    pub engines: EngineLabels,
    pub timings: StepMillis,
    pub source: Option<String>,
    /// `None` for non-command rows or command details written by an incompatible newer build.
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
    pub style: Option<AppTarget>,
    /// `(display_name, bundle_id)` of configured apps under Prompts. Matching by recorded app name as well as stored
    /// target lets removing an app reclassify its past rows as Other and re-adding it restore them without rewriting
    /// history.
    pub configured_apps: Vec<(String, String)>,
    pub disabled_builtin_styles: Vec<AppTarget>,
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
