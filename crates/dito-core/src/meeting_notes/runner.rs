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

use super::prompt::{
    Notes, chunks, fold_request, newest_within, read_points, read_refresh, refresh_request, split_window,
};
use super::schedule::{Poll, Scheduler};
use super::{RECENT_WINDOW_MS, schedule};
use crate::chat::Limits;
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
    /// The chat model is not a local one. The notes never go to a cloud model.
    NeedsLocalModel,
    /// The local chat model could not be started.
    ModelFailed(String),
}

/// The local chat model, ready to answer, with what the request must respect.
pub struct NotesModel {
    pub model: Arc<dyn TextModel>,
    pub limits: Limits,
    /// The model can switch its reasoning off, which a summary has no use for.
    pub thinking_off: bool,
    /// Held while the request runs, so the model stays loaded under it.
    pub hold: Option<Box<dyn Send + Sync>>,
}

/// What asking for the model returns.
pub type ModelFuture<'a> = Pin<Box<dyn Future<Output = Result<NotesModel, Unavailable>> + Send + 'a>>;

/// Where the model comes from. The app implements it over its chat settings and refuses a cloud model.
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
    pub me: String,
    pub others: String,
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

/// Needs a Tokio runtime. `seed` is the notes an earlier panel of the same meeting left, so the folded points are not
/// written again.
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

/// Older speech not yet folded goes to a refresh only up to this much, so a long meeting never makes the first notes
/// wait for a long read.
const OLDER_CHARS: usize = 4_000;

/// One fold request reads at most this much speech, so each request is short however long the meeting is.
const FOLD_CHUNK_CHARS: usize = 6_000;

/// One refresh. The topic and the recent lines come first and are published at once, so the notes appear without
/// waiting for anything older. Then, when a window's worth of speech has aged out of the newest five minutes, it is
/// folded into the earlier points in short requests, each published as it lands.
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
    let names = SpeakerNames { me: &deps.me, others: &deps.others };
    let budget = (ready.limits.context_chars / 2).max(2_000);
    let covered = context.covered_ms();
    let earlier = previous.map(|n| n.earlier.clone()).unwrap_or_default();
    let span = previous.and_then(|n| n.earlier_span_ms);
    let folded_until = span.map_or(0, |(_, until)| until);
    let window = split_window(&context.utterances, covered, folded_until);

    let older = newest_within(&window.unfolded, budget.min(OLDER_CHARS));
    let recent = newest_within(&window.recent, budget);
    let request = refresh_request(context, &earlier, &older, &recent, names, deps.offset_secs, ready.thinking_off);
    let reply = ask(&ready, &request).await.ok_or(Failure::Request)?;
    let (topic, lines) = read_refresh(&reply).ok_or(Failure::Request)?;
    let mut notes = Notes {
        topic,
        recent: lines,
        earlier: earlier.clone(),
        earlier_span_ms: span,
        covered_ms: covered,
        started_ts: context.started_ts,
    };
    publish(view, NotesPhase::Working { loading_model: false }, Some(notes.clone()), false);

    if window.from_ms.saturating_sub(folded_until) >= RECENT_WINDOW_MS && !window.unfolded.is_empty() {
        let start = span.map_or(window.unfolded[0].start_ms, |(from, _)| from);
        let mut points = earlier;
        for chunk in chunks(&window.unfolded, budget.min(FOLD_CHUNK_CHARS)) {
            let request = fold_request(context, &points, &chunk, names, deps.offset_secs, ready.thinking_off);
            let Some(next) = ask(&ready, &request).await.and_then(|reply| read_points(&reply)) else { break };
            points = next;
            let until = chunk.iter().map(|u| u.end_ms).max().unwrap_or(start);
            notes.earlier.clone_from(&points);
            notes.earlier_span_ms = Some((start, until));
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
