//! The records the store reads and writes.

use crate::config::EngineLabels;
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
    /// (local date `YYYY-MM-DD`, characters), ascending.
    pub per_day: Vec<(String, u64)>,
    /// (app name, recordings), descending.
    pub apps: Vec<(String, u64)>,
    /// (language code, recordings), descending.
    pub langs: Vec<(String, u64)>,
}
