//! The actor behind the notes panel: it looks at the meeting about every two seconds, asks the scheduler whether a
//! refresh is due, runs it, and publishes the result. It awaits each request before looking again, so two
//! refreshes never run at once, and it ends with its handle, taking an unfinished request with it.

use std::future::Future;
use std::hash::{Hash, Hasher};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::Instant;
use tokio_util::sync::{CancellationToken, DropGuard};

use super::lang::{Judge, meeting_lang};
use super::prompt::{
    Budget, Frame, Hint, Memo, Notes, chunks, hint_request, hint_window, memo_request, read_hint, read_memo,
    split_window,
};
use super::schedule::{Poll, Scheduler};
use super::{RECENT_WINDOW_MS, schedule};
use crate::chat::Limits;
use crate::i18n::Lang;
use crate::provider::{ChatCancel, TextModel};
use crate::store::Store;
use crate::transcription::{ActiveRecording, MeetingContext, RecordingPhase, SpeakerNames, TranscriptionStatus};

/// How often the meeting is looked at, and how long a refresh waits for the one before it.
#[derive(Debug, Clone, Copy)]
pub struct Cadence {
    pub look_every: Duration,
    pub refresh_every: Duration,
}

impl Default for Cadence {
    fn default() -> Self {
        Self { look_every: Duration::from_secs(2), refresh_every: schedule::REFRESH_EVERY }
    }
}

/// A request that has not answered by then is given up; the previous notes stay.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Why there are no notes to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unavailable {
    /// The chat model could not be started.
    ModelFailed(String),
}

/// The chat model, ready to answer, with what the request must respect.
pub struct NotesModel {
    pub model: Arc<dyn TextModel>,
    pub limits: Limits,
    /// The model can switch its reasoning off, which a summary has no use for.
    pub thinking_off: bool,
    /// The output limit the model's settings name, which a model that thinks keeps when it is the larger one.
    pub max_tokens: Option<u32>,
    /// Held while the request runs, so the model stays loaded under it.
    pub hold: Option<Box<dyn Send + Sync>>,
}

/// What asking for the model returns.
pub type ModelFuture<'a> = Pin<Box<dyn Future<Output = Result<NotesModel, Unavailable>> + Send + 'a>>;

/// Where the model comes from. The app implements it over its chat settings, local or cloud.
pub trait NotesModelSource: Send + Sync {
    /// The model is not in memory yet, so asking for it takes a while.
    fn loading(&self) -> bool {
        false
    }

    fn model(&self) -> ModelFuture<'_>;
}

pub struct NotesDeps {
    pub store: Arc<Store>,
    pub status: watch::Receiver<TranscriptionStatus>,
    pub source: Arc<dyn NotesModelSource>,
    /// The speakers' names in the interface language, for a meeting whose language has no name.
    pub me: String,
    pub others: String,
    /// The language the user set as the one they speak, if any: the notes' language while too little was said to
    /// tell, never the interface language.
    pub prior: Option<Lang>,
    /// The local time zone's offset, for the time marks of the transcript.
    pub offset_secs: i32,
    pub cadence: Cadence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotesPhase {
    /// Nothing has been said yet.
    Waiting,
    /// A refresh is running; `loading_model` while the model is still being brought into memory.
    Working {
        loading_model: bool,
    },
    Ready,
    Unavailable(Unavailable),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotesView {
    pub phase: NotesPhase,
    /// The last notes that were written. They stay through a failed refresh.
    pub notes: Option<Notes>,
    /// The last refresh failed, so `notes` may be a step behind.
    pub stale: bool,
    /// Counts publications, so a view redraws only for news.
    pub seq: u64,
}

/// The running notes. Dropping it ends the work.
pub struct NotesHandle {
    view: watch::Receiver<NotesView>,
    _end: DropGuard,
}

impl NotesHandle {
    pub fn view(&self) -> NotesView {
        self.view.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<NotesView> {
        self.view.clone()
    }
}

/// Needs a Tokio runtime. `seed` is the notes an earlier panel of the same meeting left, so the memo is not written
/// again and the meeting keeps its language.
pub fn spawn(deps: NotesDeps, meeting: i64, seed: Option<Notes>) -> NotesHandle {
    let initial = NotesView { phase: NotesPhase::Waiting, notes: seed.clone(), stale: false, seq: 0 };
    let (tx, rx) = watch::channel(initial);
    let cancel = CancellationToken::new();
    tokio::spawn(actor(deps, meeting, seed, tx, cancel.clone()));
    NotesHandle { view: rx, _end: cancel.drop_guard() }
}

fn publish(tx: &watch::Sender<NotesView>, phase: NotesPhase, notes: Option<Notes>, stale: bool) {
    tx.send_modify(|view| {
        view.phase = phase;
        if notes.is_some() {
            view.notes = notes;
        }
        view.stale = stale;
        view.seq += 1;
    });
}

fn fingerprint(context: &MeetingContext) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (context.utterances.len(), context.covered_ms()).hash(&mut hasher);
    hasher.finish()
}

async fn actor(
    deps: NotesDeps,
    meeting: i64,
    seed: Option<Notes>,
    view: watch::Sender<NotesView>,
    cancel: CancellationToken,
) {
    let begun = Instant::now();
    let mut scheduler = Scheduler::new(deps.cadence.refresh_every);
    let mut notes = seed;
    let mut status = deps.status.clone();
    loop {
        let (context, active) = {
            let current = status.borrow_and_update().clone();
            let store = deps.store.clone();
            let read = tokio::task::spawn_blocking({
                let current = current.clone();
                move || MeetingContext::read(&store, &current, meeting)
            })
            .await;
            match read {
                Ok(Ok(Some(context))) => (context, ActiveRecording::of(&current).filter(|a| a.id == meeting)),
                Ok(Ok(None)) => return,
                Ok(Err(e)) => {
                    tracing::warn!("meeting notes could not read the meeting: {e}");
                    (MeetingContext::empty(meeting), ActiveRecording::of(&current).filter(|a| a.id == meeting))
                }
                Err(_) => return,
            }
        };
        // The notes' last refresh comes once the session is over, with the text the meeting ended with.
        let ending = active.is_none();
        let poll = Poll {
            now: begun.elapsed(),
            fingerprint: fingerprint(&context),
            has_text: !context.utterances.is_empty(),
            paused: active.is_some_and(|a| a.phase == RecordingPhase::Paused),
            ending,
        };
        let ran = scheduler.due(&poll);
        if ran {
            scheduler.started(poll.now);
            publish(&view, NotesPhase::Working { loading_model: deps.source.loading() }, None, false);
            let outcome = tokio::select! {
                outcome = refresh(&deps, &context, notes.as_ref(), &view) => outcome,
                () = cancel.cancelled() => return,
            };
            match outcome {
                Ok(written) => {
                    scheduler.succeeded(poll.fingerprint);
                    notes = Some(written.clone());
                    publish(&view, NotesPhase::Ready, Some(written), false);
                }
                Err(Failure::Unavailable(why)) => publish(&view, NotesPhase::Unavailable(why), None, notes.is_some()),
                Err(Failure::Request) => publish(&view, NotesPhase::Ready, None, true),
            }
        } else if !poll.has_text && view.borrow().phase != NotesPhase::Waiting {
            publish(&view, NotesPhase::Waiting, None, false);
        }
        if ending && (ran || !poll.has_text || scheduler.covers(poll.fingerprint)) {
            return;
        }
        tokio::select! {
            changed = status.changed() => if changed.is_err() { return },
            () = tokio::time::sleep(deps.cadence.look_every) => {}
            () = cancel.cancelled() => return,
        }
    }
}

enum Failure {
    Unavailable(Unavailable),
    Request,
}

/// One memo request reads at most this much speech, so each request is short however long the meeting is.
const MEMO_CHUNK_CHARS: usize = 6_000;

/// One refresh. The hint on the last minute comes first and is published at once, so the notes appear without
/// waiting for anything older. Then, when five minutes of speech have aged out of the newest five minutes, the memo
/// takes them in, in short requests, each published as it lands.
async fn refresh(
    deps: &NotesDeps,
    context: &MeetingContext,
    previous: Option<&Notes>,
    view: &watch::Sender<NotesView>,
) -> Result<Notes, Failure> {
    let ready = deps.source.model().await.map_err(Failure::Unavailable)?;
    if deps.source.loading() {
        publish(view, NotesPhase::Working { loading_model: false }, None, false);
    }
    let ui_names = SpeakerNames { me: &deps.me, others: &deps.others };
    let budget = Budget { thinking_off: ready.thinking_off, configured: ready.max_tokens };
    let chars = (ready.limits.context_chars / 2).max(2_000);
    let covered = context.covered_ms();
    let mut notes = previous.cloned().unwrap_or_default();
    notes.started_ts = context.started_ts;

    let window = hint_window(&context.utterances, covered);
    let lang = meeting_lang(&window, notes.lang, deps.prior, Judge::Hint);
    let frame = Frame { context, lang: lang.request, ui_names, offset: deps.offset_secs };
    let request = hint_request(&frame, &window, notes.now.as_ref(), budget);
    let reply = ask(&ready, &request).await.ok_or(Failure::Request)?;
    let topic = read_hint(&reply).map_err(|e| {
        tracing::warn!("meeting notes hint was not taken: {e:?}");
        Failure::Request
    })?;
    notes.lang = lang.settled;
    notes.now = Some(Hint { topic, covered_ms: covered });
    publish(view, NotesPhase::Working { loading_model: false }, Some(notes.clone()), false);

    let memo_until = notes.memo.covered_ms;
    let aged = split_window(&context.utterances, covered, memo_until);
    if aged.from_ms.saturating_sub(memo_until) >= RECENT_WINDOW_MS && !aged.unfolded.is_empty() {
        let start = notes.memo.span_ms.map_or(aged.unfolded[0].start_ms, |(from, _)| from);
        for (n, chunk) in chunks(&aged.unfolded, chars.min(MEMO_CHUNK_CHARS)).into_iter().enumerate() {
            let lang = meeting_lang(&chunk, notes.lang, deps.prior, Judge::Memo);
            let frame = Frame { context, lang: lang.request, ui_names, offset: deps.offset_secs };
            let request = memo_request(&frame, &chunk, &notes.memo, budget);
            let Some(reply) = ask(&ready, &request).await else { break };
            let memo = match read_memo(&reply) {
                Ok(memo) if memo.is_empty() && !notes.memo.is_empty() => {
                    tracing::warn!("meeting notes memo chunk {n} came back empty; the memo stays as it was");
                    break;
                }
                Ok(memo) => memo,
                Err(e) => {
                    tracing::warn!("meeting notes memo chunk {n} was not taken: {e:?}; the memo stays as it was");
                    break;
                }
            };
            let until = chunk.iter().map(|u| u.end_ms).max().unwrap_or(start);
            notes.lang = lang.settled;
            notes.memo = Memo { span_ms: Some((start, until)), covered_ms: until, ..memo };
            publish(view, NotesPhase::Working { loading_model: false }, Some(notes.clone()), false);
        }
    }
    Ok(notes)
}

async fn ask(ready: &NotesModel, request: &crate::provider::ChatRequest) -> Option<String> {
    let cancel = ChatCancel::default();
    let mut ignore = |_: &str| {};
    match tokio::time::timeout(REQUEST_TIMEOUT, ready.model.chat(request, &mut ignore, &cancel)).await {
        Ok(Ok(reply)) => Some(reply),
        Ok(Err(e)) => {
            tracing::warn!("meeting notes request failed: {e}");
            None
        }
        Err(_) => {
            cancel.cancel();
            tracing::warn!("meeting notes request timed out");
            None
        }
    }
}

#[cfg(test)]
mod tests;
