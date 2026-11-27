//! One reply being written: the request runs on the runtime, its pieces land in a shared buffer the page reads on a
//! timer, and the runtime task stores the outcome itself, so a reply is never left unsaved when the window goes away.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use dito_core::chat;
use dito_core::config::{EngineConfig, Source};
use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;

use dito_core::provider::{ChatCancel, ChatRequest};
use dito_core::store::{Author, ChatMessageRow, MessageState, NewChatMessage, StoreError};
use tokio::sync::Notify;

use crate::services;
use crate::state::AppState;
use crate::timefmt::{date_time, now};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Finished {
    pub content: String,
    pub state: MessageState,
    /// The encoded error line of a failure, as the history stores them.
    pub error: Option<String>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Adds the reply's row to the conversation `rows` of chat `chat_id` and starts the request for it: the one path
/// every reply takes, whether the Chat page or the meeting notes asked.
pub(crate) fn begin(
    st: &Arc<AppState>,
    chat_id: i64,
    rows: &[ChatMessageRow],
) -> Result<(ChatMessageRow, Generation), StoreError> {
    let facts = services::chat_model_facts(st);
    let off = dito_platform::local_utc_offset_secs();
    let mut request = chat::request_for(rows, move |ts| date_time(ts, off), facts.limits.context_chars);
    request.thinking = chat::thinking_request(services::chat_thinking_choice(st, &facts));
    let model = services::chat_model_label(st);
    let ts = now();
    let placeholder = NewChatMessage {
        author: Author::Assistant,
        content: "",
        state: MessageState::Generating,
        attachments: &[],
        model: Some(&model),
        ts,
    };
    let id = st.store.add_chat_message(chat_id, &placeholder)?;
    let row = ChatMessageRow {
        id,
        author: Author::Assistant,
        content: String::new(),
        state: MessageState::Generating,
        error: None,
        attachments: Vec::new(),
        model: Some(model),
        ts,
    };
    Ok((row, Generation::start(st, chat_id, id, request)))
}

pub(crate) struct Generation {
    pub chat_id: i64,
    pub message_id: i64,
    pub started: Instant,
    /// When the model was ready to answer; `None` while it still loads.
    ready: Arc<Mutex<Option<Instant>>>,
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
        let engine = st.cfg.get().chat_engine().into_owned();
        let loads = engine.source == Source::Local && st.local.status(&engine.local_model) != EngineStatus::Running;
        let ready = Arc::new(Mutex::new((!loads).then(Instant::now)));
        let task = Job {
            st: st.clone(),
            engine,
            message_id,
            request,
            buffer: buffer.clone(),
            cancel: cancel.clone(),
            ready: ready.clone(),
        };
        let (stop_wait, done_slot) = (stop.clone(), done.clone());
        st.rt.spawn(async move {
            let _busy = crate::services::chat_reply_started(&task.st);
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
        Self { chat_id, message_id, started: Instant::now(), ready, buffer, seen: 0, cancel, stop, done }
    }

    /// A reply that is being written and never ends, with `written` already on screen and the sweep `ms` into its loop.
    #[cfg(feature = "screenshots")]
    pub fn posed(chat_id: i64, message_id: i64, written: &str, ms: u64) -> Self {
        Self {
            chat_id,
            message_id,
            started: Instant::now() - std::time::Duration::from_millis(ms),
            ready: Arc::new(Mutex::new(Some(Instant::now() - std::time::Duration::from_millis(ms)))),
            buffer: Arc::new(Mutex::new(written.to_owned())),
            seen: written.len(),
            cancel: ChatCancel::default(),
            stop: Arc::new(Notify::new()),
            done: Arc::new(Mutex::new(None)),
        }
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

    pub fn ready(&self) -> Option<Instant> {
        *lock(&self.ready)
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
    engine: EngineConfig,
    message_id: i64,
    request: ChatRequest,
    buffer: Arc<Mutex<String>>,
    cancel: ChatCancel,
    ready: Arc<Mutex<Option<Instant>>>,
}

impl Job {
    async fn run(&self, stop: &Notify) -> Finished {
        let written = || lock(&self.buffer).clone();
        let model = self.st.factory.text_model_for(&self.engine).await;
        lock(&self.ready).get_or_insert_with(Instant::now);
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
