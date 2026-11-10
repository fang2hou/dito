//! Re-running recognition or polish on saved history entries, updating the stored row.

use std::sync::Arc;

use anyhow::Result;
use dito_core::audio::AudioClip;
use dito_core::factory::Engines;
use dito_core::pipeline::{self, CancellationToken, Job, PipelineError, PolishJob};
use dito_core::store::HistoryEntry;

use crate::state::AppState;

/// Fetch one history row by id, off the UI thread.
pub async fn history_entry(st: Arc<AppState>, id: i64) -> Result<Option<HistoryEntry>> {
    let store = st.store.clone();
    st.rt
        .spawn_blocking(move || store.get_history(id))
        .await
        .map_err(|e| anyhow::anyhow!("history lookup task failed: {e}"))?
        .map_err(Into::into)
}

/// Delete a history row and any audio file it kept, preserving the lookup → file removal →
/// row delete order. Off the UI thread.
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

/// Run recognition + polish again on a saved recording and update the history entry.
pub async fn recognize_again(st: Arc<AppState>, entry: HistoryEntry) -> Result<(), String> {
    let path = entry.audio_path.clone().ok_or("no audio was kept for this entry")?;
    // File read + WAV decode are synchronous; keep them off the async executor.
    let clip = st
        .rt
        .spawn_blocking(move || {
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            AudioClip::from_wav(&bytes).ok_or_else(|| "the saved audio could not be read".to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
    let st2 = st.clone();
    st.rt
        .spawn(async move {
            let cfg = st2.cfg.get();
            let store = st2.store.clone();
            let words = st2
                .rt
                .spawn_blocking(move || store.words(None).unwrap_or_default())
                .await
                .map_err(|e| e.to_string())?;
            let engines: &dyn Engines = st2.factory.as_ref();
            let out = pipeline::run(
                engines,
                &cfg,
                &Job { clip: &clip, style: entry.style.clone(), dictionary: &words },
                &|_| {},
                &CancellationToken::new(),
            )
            .await
            .map_err(|e| match e {
                PipelineError::Cancelled => "cancelled".to_string(),
                PipelineError::Failed(e) => e.to_string(),
            })?;
            let (id, raw, polished, lang) = (entry.id, out.raw, out.polished, out.lang.code().to_string());
            let labels = cfg.engine_labels();
            let store = st2.store.clone();
            st2.rt
                .spawn_blocking(move || store.complete_history(id, &raw, polished.as_deref(), &lang, &labels))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
}

/// Polish the stored original text again with the current prompts.
pub async fn polish_again(st: Arc<AppState>, entry: HistoryEntry) -> Result<(), String> {
    if entry.raw.trim().is_empty() {
        return Err("there is no recognized text to polish".into());
    }
    let st2 = st.clone();
    st.rt
        .spawn(async move {
            let cfg = st2.cfg.get();
            let polisher = st2.factory.polisher_for(&cfg.polish.engine).await.map_err(|e| e.to_string())?;
            let store = st2.store.clone();
            let words = st2
                .rt
                .spawn_blocking(move || store.words(None).unwrap_or_default())
                .await
                .map_err(|e| e.to_string())?;
            let polish_job = PolishJob {
                prompts: &cfg.prompts,
                style: entry.style.clone(),
                text: &entry.raw,
                dictionary: pipeline::polish_dictionary(&cfg, &words),
                lang: None,
            };
            let text = pipeline::polish_text(polisher.as_ref(), polish_job).await.map_err(|e| e.to_string())?;
            let (id, text) = (entry.id, text);
            let polish_engine = cfg.polish.engine.label();
            let store = st2.store.clone();
            st2.rt
                .spawn_blocking(move || store.set_polished(id, &text, &polish_engine))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
}
