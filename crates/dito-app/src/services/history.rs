//! Re-running transcription or polish on saved history entries, updating the stored row.

use std::sync::Arc;

use anyhow::Result;
use dito_core::controller::{self, HistoryRerunError};
use dito_core::store::HistoryEntry;

use crate::state::AppState;

pub async fn history_entry(st: Arc<AppState>, id: i64) -> Result<Option<HistoryEntry>> {
    let store = st.store.clone();
    st.rt
        .spawn_blocking(move || store.get_history(id))
        .await
        .map_err(|e| anyhow::anyhow!("history lookup task failed: {e}"))?
        .map_err(Into::into)
}

/// Delete a history row and any audio file it kept. Order matters: look up the audio path, remove the file, then
/// delete the row.
pub async fn delete_history_entry(st: Arc<AppState>, id: i64) -> Result<()> {
    let store = st.store.clone();
    st.rt
        .spawn_blocking(move || {
            if let Some(path) = store.get_history(id)?.and_then(|e| e.audio_path) {
                let _ = std::fs::remove_file(path);
            }
            store.delete_history(id)
        })
        .await
        .map_err(|e| anyhow::anyhow!("history delete task failed: {e}"))?
        .map_err(Into::into)
}

pub async fn transcribe_again(st: Arc<AppState>, entry: HistoryEntry) -> Result<(), HistoryRerunError> {
    let cfg = st.cfg.get();
    let factory = st.factory.clone();
    let store = st.store.clone();
    st.rt.spawn(async move { controller::transcribe_again(factory.as_ref(), store, &cfg, entry).await }).await??;
    Ok(())
}

pub async fn polish_again(st: Arc<AppState>, entry: HistoryEntry) -> Result<(), HistoryRerunError> {
    let cfg = st.cfg.get();
    let factory = st.factory.clone();
    let store = st.store.clone();
    st.rt.spawn(async move { controller::polish_again(factory.as_ref(), store, &cfg, entry).await }).await??;
    Ok(())
}
