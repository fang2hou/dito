//! Keeps a separate local chat model in memory only as long as its loading choice asks
//! (`design:settings/local-models/chat-model`): always, or from opening the chat until a while after leaving it.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dito_core::config::{ChatLoading, Config, Source};

use crate::state::AppState;

/// Coming back within this long keeps the model, so switching pages does not reload it.
const RELEASE_AFTER: Duration = Duration::from_secs(60);
const BUSY_RECHECK: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct ChatModelKeeper {
    open: AtomicBool,
    /// Bumped by every open and leave, so a release timer older than the latest one does nothing.
    epoch: AtomicU64,
    /// Replies in flight: a model is never released under one.
    busy: Arc<AtomicUsize>,
    /// The separate chat model last seen, so a replaced one can be released.
    last: Mutex<Option<String>>,
}

/// Held while a reply runs.
pub struct ReplyGuard(Arc<AtomicUsize>);

impl Drop for ReplyGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

pub fn chat_reply_started(st: &AppState) -> ReplyGuard {
    st.chat_model.busy.fetch_add(1, Ordering::SeqCst);
    ReplyGuard(st.chat_model.busy.clone())
}

pub fn chat_opened(st: &Arc<AppState>) {
    let keeper = &st.chat_model;
    keeper.open.store(true, Ordering::SeqCst);
    keeper.epoch.fetch_add(1, Ordering::SeqCst);
    if let Some(id) = on_demand_model(&st.cfg.get()) {
        warm(st, id);
    }
}

pub fn chat_left(st: &Arc<AppState>) {
    let keeper = &st.chat_model;
    keeper.open.store(false, Ordering::SeqCst);
    let epoch = keeper.epoch.fetch_add(1, Ordering::SeqCst) + 1;
    let st = st.clone();
    st.rt.clone().spawn(async move {
        tokio::time::sleep(RELEASE_AFTER).await;
        loop {
            if st.chat_model.epoch.load(Ordering::SeqCst) != epoch {
                return;
            }
            if st.chat_model.busy.load(Ordering::SeqCst) == 0 {
                break;
            }
            tokio::time::sleep(BUSY_RECHECK).await;
        }
        if let Some(id) = on_demand_model(&st.cfg.get()) {
            release(&st, id);
        }
    });
}

/// Follows every settings change: a replaced chat model is released, and one that now loads only on demand is
/// released while the chat is closed or loaded while it is open. Keeping a model always loaded is
/// `local_models_in_use`'s job.
pub fn follow_chat_model(st: &Arc<AppState>) {
    let (st, mut cfg) = (st.clone(), st.cfg.subscribe());
    st.rt.clone().spawn(async move {
        loop {
            let config = cfg.borrow_and_update().clone();
            let current = config.separate_chat_model().map(str::to_owned);
            let previous = std::mem::replace(&mut *lock(&st.chat_model.last), current.clone());
            if let Some(previous) = previous.filter(|p| current.as_ref() != Some(p)) {
                release_unless_used(&st, &config, previous);
            }
            if let Some(id) = on_demand_model(&config) {
                if st.chat_model.open.load(Ordering::SeqCst) {
                    warm(&st, id);
                } else if st.chat_model.busy.load(Ordering::SeqCst) == 0 {
                    release_unless_used(&st, &config, id.to_owned());
                }
            }
            if cfg.changed().await.is_err() {
                break;
            }
        }
    });
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn on_demand_model(cfg: &Config) -> Option<String> {
    (cfg.chat.loading == ChatLoading::WhileOpen).then(|| cfg.separate_chat_model().map(str::to_owned)).flatten()
}

fn warm(st: &Arc<AppState>, id: String) {
    let local = st.local.clone();
    st.rt.spawn(async move { local.warm(&id).await });
}

fn release(st: &Arc<AppState>, id: String) {
    release_unless_used(st, &st.cfg.get(), id);
}

/// A model dictation also uses stays: only the chat's own model is released.
fn release_unless_used(st: &Arc<AppState>, cfg: &Config, id: String) {
    let used = cfg.local_models_in_use().contains(&id)
        || (cfg.text_model.source == Source::Local && cfg.text_model.local_model == id)
        || (cfg.asr.source == Source::Local && cfg.asr.local_model == id);
    if !used {
        let local = st.local.clone();
        st.rt.spawn_blocking(move || local.unload(&id));
    }
}
