//! What the editor keeps for each cloud speech configuration it has shown, by id.

use dito_core::config::PreferredLanguage;
use gpui_kit::*;

use crate::ui::{self, EditorState};

/// The editing state of one configuration: the request JSON as typed with its undo history, the key as typed, and where
/// validation and testing stand. It lives from the first time the configuration is shown until the configuration is
/// deleted, so choosing another tile, reordering the tiles or opening the documentation never loses an edit, a result
/// or the history.
pub(super) struct Draft {
    pub source: Entity<EditorState>,
    /// The key as typed, and as the Keychain holds it.
    pub key: String,
    pub saved_key: String,
    /// Why the JSON cannot be used yet; `None` while it validates.
    pub error: Option<String>,
    /// Typed, not checked yet.
    pub pending: bool,
    /// Counts the edits: a check or a test result belongs to the edit it started from.
    pub revision: u64,
    pub test: ui::TestState,
    pub tested_language: Option<PreferredLanguage>,
    /// Which palette the highlighter was built for.
    pub styled_dark: Option<bool>,
    /// The check that runs once typing pauses. A new edit replaces it, which cancels the one before.
    pub check: Option<Task<()>>,
}
