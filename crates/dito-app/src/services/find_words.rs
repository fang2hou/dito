//! Runs the word finder on the runtime and hands the UI its progress and result.

use std::path::PathBuf;
use std::sync::Arc;

use dito_core::word_finder::{self, Candidate, Progress};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordSearchError {
    /// The chosen file is missing, of another type than its name says, or not text.
    File,
    /// The text model is not available or its request failed.
    Model,
}

/// Dropping it stops the search.
pub struct WordSearch {
    pub progress: watch::Receiver<Progress>,
    task: JoinHandle<Result<Vec<Candidate>, WordSearchError>>,
}

impl WordSearch {
    pub async fn finished(&mut self) -> Result<Vec<Candidate>, WordSearchError> {
        (&mut self.task).await.unwrap_or(Err(WordSearchError::Model))
    }
}

impl Drop for WordSearch {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Reads `file` after `text`, as one passage.
pub fn find_words(st: Arc<AppState>, text: String, file: Option<PathBuf>) -> WordSearch {
    let (progress_tx, progress) = watch::channel(Progress::default());
    let rt = st.rt.clone();
    let task = rt.spawn(async move {
        let mut text = text;
        if let Some(path) = file {
            let read = st.rt.spawn_blocking(move || word_finder::read_source(&path)).await;
            let Ok(Ok(file_text)) = read else {
                tracing::warn!("word finder: the file could not be read: {read:?}");
                return Err(WordSearchError::File);
            };
            text.push_str("\n\n");
            text.push_str(&file_text);
        }
        let store = st.store.clone();
        let known = st.rt.spawn_blocking(move || store.words(None)).await;
        let Ok(Ok(known)) = known else {
            tracing::warn!("word finder: the wordbook could not be read");
            return Err(WordSearchError::Model);
        };
        let cfg = st.cfg.get();
        let mut hud = st.dictation.hud();
        let found = async {
            let text_model = st.factory.text_model_for(&cfg.text_model).await?;
            word_finder::find_words(text_model.as_ref(), &text, &known, &mut hud, &progress_tx).await
        }
        .await;
        found.map_err(|e| {
            tracing::warn!("word finder: {e}");
            WordSearchError::Model
        })
    });
    WordSearch { progress, task }
}
