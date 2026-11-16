//! One reply being written: the request runs on the runtime, its pieces land in a shared buffer the page reads on a
//! timer, and the runtime task stores the outcome itself, so a reply is never left unsaved when the window goes away.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use dito_core::provider::{ChatCancel, ChatRequest};
use dito_core::store::MessageState;
use tokio::sync::Notify;

use crate::state::AppState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Finished {
    pub content: String,
    pub state: MessageState,
    /// The encoded error line of a failure, as the history stores them.
    pub error: Option<String>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(super) struct Generation {
    pub chat_id: i64,
    pub message_id: i64,
    pub started: Instant,
    buffer: Arc<Mutex<String>>,
    seen: usize,
    cancel: ChatCancel,
    stop: Arc<Notify>,
    done: Arc<Mutex<Option<Finished>>>,
}

impl Generation {
    pub fn start(st: &Arc<AppState>, chat_id: i64, message_id: i64, request: ChatRequest) -> Self {
        let buffer = Arc::new(Mutex::new(String::new()));
        let cancel = ChatCancel::default();
        let stop = Arc::new(Notify::new());
        let done = Arc::new(Mutex::new(None));
        let task = Job { st: st.clone(), message_id, request, buffer: buffer.clone(), cancel: cancel.clone() };
        let (stop_wait, done_slot) = (stop.clone(), done.clone());
        st.rt.spawn(async move {
            let finished = task.run(&stop_wait).await;
            let stored = task.st.store.finish_chat_message(
                message_id,
                &finished.content,
                finished.state,
                finished.error.as_deref(),
            );
            if let Err(e) = stored {
                tracing::warn!("could not save a chat reply: {e}");
            }
            *lock(&done_slot) = Some(finished);
        });
        Self { chat_id, message_id, started: Instant::now(), buffer, seen: 0, cancel, stop, done }
    }

    /// Asks the engine to stop between tokens and ends a wait that cannot be interrupted; what was written is kept.
    pub fn stop(&self) {
        self.cancel.cancel();
        self.stop.notify_one();
    }

    /// The reply so far, when it has grown since the last call.
    pub fn grown(&mut self) -> Option<String> {
        let buffer = lock(&self.buffer);
        (buffer.len() != self.seen).then(|| {
            self.seen = buffer.len();
            buffer.clone()
        })
    }

    pub fn finished(&self) -> Option<Finished> {
        lock(&self.done).clone()
    }
}

impl Drop for Generation {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Job {
    st: Arc<AppState>,
    message_id: i64,
    request: ChatRequest,
    buffer: Arc<Mutex<String>>,
    cancel: ChatCancel,
}

impl Job {
    async fn run(&self, stop: &Notify) -> Finished {
        let written = || lock(&self.buffer).clone();
        let model = {
            let cfg = self.st.cfg.get();
            self.st.factory.text_model_for(&cfg.text_model).await
        };
        let model = match model {
            Ok(model) => model,
            Err(e) => return failed(written(), &e.to_string()),
        };
        let buffer = self.buffer.clone();
        let mut on_delta = move |piece: &str| lock(&buffer).push_str(piece);
        let reply = tokio::select! {
            reply = model.chat(&self.request, &mut on_delta, &self.cancel) => Some(reply),
            () = stop.notified() => None,
        };
        tracing::debug!(target: "dito_trace", message = self.message_id, "chat reply ended");
        match reply {
            Some(Ok(text)) => {
                let state = if self.cancel.is_cancelled() { MessageState::Stopped } else { MessageState::Done };
                Finished { content: text, state, error: None }
            }
            Some(Err(e)) => failed(written(), &e.to_string()),
            None => Finished { content: written(), state: MessageState::Stopped, error: None },
        }
    }
}

fn failed(content: String, error: &str) -> Finished {
    Finished { content, state: MessageState::Failed, error: Some(error.to_owned()) }
}
