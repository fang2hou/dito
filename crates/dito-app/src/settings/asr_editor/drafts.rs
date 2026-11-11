//! What the editor keeps for each cloud speech configuration it has shown, by id.

use dito_core::config::PreferredLanguage;
use gpui_kit::*;

use crate::ui::{self, EditorState};

/// Lives from the first time the configuration is shown until it is deleted, so choosing another tile, reordering the
/// tiles or opening the documentation never loses an edit, a result or the history.
pub(super) struct Draft {
    pub source: Entity<EditorState>,
    pub key: String,
    pub saved_key: String,
    pub error: Option<String>,
    pub pending: bool,
    /// Counts the edits: a check or a test result belongs to the edit it started from.
    pub revision: u64,
    pub test: ui::TestState,
    pub tested_language: Option<PreferredLanguage>,
    pub styled_dark: Option<bool>,
    /// A new edit replaces it, which cancels the one before.
    pub check: Option<Task<()>>,
}
