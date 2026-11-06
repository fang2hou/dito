use std::sync::Arc;
use std::time::Duration;

use dito_core::controller::{Command, HudState};
use dito_core::store::HistoryEntry;
use gpui_kit::*;

use crate::state::AppState;

/// Headless end-to-end check: key down → finish → wait for delivery → print what was stored.
pub fn run(st: Arc<AppState>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut hud = st.dictation.hud();
        let newest_before = latest_history_entry(&st).map(|entry| entry.id);
        cx.background_executor().timer(Duration::from_millis(500)).await;
        st.dictation.send(Command::KeyDown);
        cx.background_executor().timer(Duration::from_millis(400)).await;
        st.dictation.send(Command::Finish);
        let mut seen = Vec::new();
        let outcome = loop {
            if hud.changed().await.is_err() {
                break None;
            }
            let s = hud.borrow_and_update().clone();
            seen.push(format!("{s:?}"));
            if matches!(s, HudState::Inserted | HudState::Copied | HudState::Error { .. }) {
                break Some(s);
            }
        };
        println!("HUD states: {}", seen.join(" -> "));
        // The controller writes history on a blocking thread after reporting the outcome.
        let mut latest = None;
        for _ in 0..20 {
            latest = latest_history_entry(&st).filter(|entry| Some(entry.id) != newest_before);
            if latest.is_some() {
                break;
            }
            cx.background_executor().timer(Duration::from_millis(100)).await;
        }
        match latest {
            Some(e) => println!(
                "history: kind={:?} app={:?} raw={:?} polished={:?} error={:?}",
                e.kind, e.app, e.raw, e.polished, e.error
            ),
            None => println!("history: (empty)"),
        }
        let ok = matches!(outcome, Some(HudState::Inserted | HudState::Copied));
        println!("demo-run: {}", if ok { "OK" } else { "FAILED" });
        cx.update(|cx| cx.quit());
        if !ok {
            std::process::exit(1);
        }
    })
    .detach();
}

fn latest_history_entry(st: &AppState) -> Option<HistoryEntry> {
    st.store.list_history(&Default::default()).ok().and_then(|history| history.into_iter().next())
}
