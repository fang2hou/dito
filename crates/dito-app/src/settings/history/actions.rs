use std::sync::Arc;

use dito_core::store::{HistoryEntry, HistoryFilter};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::export::history_csv;
use super::page::HistoryPage;
use crate::services;
use crate::settings::downloads_dir::downloads_dir;
use crate::settings::page_query::QueryPage;
use crate::state::AppState;

impl HistoryPage {
    pub(super) fn run_job<F, Fut>(&mut self, id: i64, cx: &mut Context<Self>, job: F)
    where
        F: FnOnce(Arc<AppState>, HistoryEntry) -> Fut + 'static,
        Fut: Future<Output = Result<(), String>> + 'static,
    {
        // Reserve before the first await: repeated clicks while the lookup runs must not
        // launch a second job for the same row.
        if !self.busy.insert(id) {
            return;
        }
        cx.notify();
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let entry = match services::history_entry(st.clone(), id).await {
                Ok(Some(e)) => e,
                other => {
                    if let Err(e) = other {
                        tracing::warn!("history action failed: {e:#}");
                    }
                    let _ = this.update(cx, |t, cx| {
                        t.busy.remove(&id);
                        cx.notify();
                    });
                    return;
                }
            };
            let r = job(st, entry).await;
            if let Err(e) = &r {
                tracing::warn!("history action failed: {e}");
            }
            let _ = this.update(cx, |t, cx| {
                t.busy.remove(&id);
                // Unbusy is visible regardless of what the follow-up reload returns.
                cx.notify();
                t.load(cx);
            });
        })
        .detach();
    }

    pub(super) fn toggle_original(&mut self, id: i64, cx: &mut Context<Self>) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
        cx.notify();
    }

    pub(super) fn copy(&self, text: &str) {
        if let Err(e) = dito_platform::insert::copy_to_clipboard(text) {
            tracing::warn!("copy failed: {e}");
        }
    }

    pub(super) fn delete(&mut self, id: i64, cx: &mut Context<Self>) {
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            if let Err(e) = services::delete_history_entry(st, id).await {
                tracing::warn!("history delete failed: {e:#}");
            }
            let _ = this.update(cx, |t, cx| t.load(cx));
        })
        .detach();
    }

    pub(super) fn export(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_new_path(&downloads_dir(), Some("dito-history.csv"));
        let st = self.st.clone();
        cx.spawn(async move |_, _| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            let rows = match services::history_list(st.clone(), HistoryFilter::default()).await {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::warn!("export: could not read the history: {e:#}");
                    return;
                }
            };
            let cfg = st.cfg.get();
            let csv = history_csv(&rows, &cfg.prompts, cfg.general.lang(), dito_platform::local_utc_offset_secs());
            match st.rt.clone().spawn_blocking(move || std::fs::write(&path, csv)).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::warn!("export: {e}"),
                Err(e) => tracing::warn!("export: {e}"),
            }
        })
        .detach();
    }
}
