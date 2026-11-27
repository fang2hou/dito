//! Runs the notes jobs serially and publishes each successful hint or memo chunk.

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
    Budget, Frame, Hint, MAX_TRAIL, Memo, Notes, ReplyError, hint_request, hint_window, memo_request, read_hint,
    read_memo,
};
use super::schedule::{self, Job, Outcome, Poll, Scheduler};
use crate::chat::Limits;
use crate::error::ProviderError;
use crate::i18n::Lang;
use crate::provider::{ChatCancel, ChatRequest, TextModel};
use crate::store::Store;
use crate::transcription::{
    ActiveRecording, MeetingContext, RecordingPhase, SpeakerNames, TranscriptionStatus, Utterance,
};

#[derive(Debug, Clone, Copy)]
pub struct Cadence {
    pub look_every: Duration,
    pub hint_every: Duration,
    pub final_gap: Duration,
}

impl Default for Cadence {
    fn default() -> Self {
        Self { look_every: Duration::from_secs(2), hint_every: schedule::HINT_EVERY, final_gap: schedule::FINAL_GAP }
    }
}

const MEMO_CHUNK_CHARS: usize = 6_000;

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
    /// Local inference gets more time to answer than a cloud request.
    pub local: bool,
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
    /// A request is running; `loading_model` while the model is still being brought into memory.
    Working {
        loading_model: bool,
    },
    Ready,
    Unavailable(Unavailable),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stale {
    /// Two or more consecutive hint failures.
    pub hint: bool,
    /// The last memo chunk failed.
    pub memo: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotesView {
    pub phase: NotesPhase,
    /// The last notes that were written. They stay through a failed request.
    pub notes: Option<Notes>,
    pub stale: Stale,
    /// The recording has ended and only the final memo may still be running.
    pub ended: bool,
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
    let ended = deps.status.borrow().state.transcript_id() != Some(meeting);
    let phase = if seed.is_some() { NotesPhase::Ready } else { NotesPhase::Waiting };
    let initial = NotesView { phase, notes: seed.clone(), stale: Stale::default(), ended, seq: 0 };
    let (tx, rx) = watch::channel(initial);
    let cancel = CancellationToken::new();
    tokio::spawn(actor(deps, meeting, seed, tx, cancel.clone()));
    NotesHandle { view: rx, _end: cancel.drop_guard() }
}

fn publish(tx: &watch::Sender<NotesView>, update: impl FnOnce(&mut NotesView)) {
    tx.send_modify(|view| {
        update(view);
        view.seq += 1;
    });
}

async fn actor(
    deps: NotesDeps,
    meeting: i64,
    seed: Option<Notes>,
    view: watch::Sender<NotesView>,
    cancel: CancellationToken,
) {
    let begun = Instant::now();
    let mut run = Run::new(seed, deps.cadence);
    let mut status = deps.status.clone();

    loop {
        let current = status.borrow_and_update().clone();
        let active = ActiveRecording::of(&current).filter(|a| a.id == meeting);
        let ending = active.is_none();
        if ending && !view.borrow().ended {
            publish(&view, |v| v.ended = true);
        }
        if run.unavailable && ending {
            return;
        }

        if !run.unavailable && !active.is_some_and(|a| a.phase == RecordingPhase::Finishing) {
            let meeting_ms = current.state.elapsed_ms();
            let store = deps.store.clone();
            let read = tokio::task::spawn_blocking(move || MeetingContext::read(&store, &current, meeting));
            let context = tokio::select! {
                read = read => match read {
                    Ok(Ok(Some(context))) => Some(context),
                    Ok(Err(error)) => {
                        tracing::warn!("meeting notes could not read the meeting: {error}");
                        None
                    }
                    Ok(Ok(None)) | Err(_) => return,
                },
                () = cancel.cancelled() => return,
            };
            if let Some(context) = context {
                let latest = ActiveRecording::of(&status.borrow()).filter(|a| a.id == meeting);
                if latest != active {
                    continue;
                }
                let covered_ms = context.covered_ms();
                let window = hint_window(&context.utterances, covered_ms);
                let fingerprint = hint_fingerprint(&window);
                let poll = run.poll(begun.elapsed(), meeting_ms, covered_ms, active, !window.is_empty(), fingerprint);

                if let Some(job) = run.scheduler.next(&poll) {
                    run.start(job, &poll, &context);
                    publish(&view, |v| v.phase = NotesPhase::Working { loading_model: deps.source.loading() });
                    let chat_cancel = ChatCancel::default();
                    let input = JobInput { job, context: &context, window: &window };
                    let result = tokio::select! {
                        biased;
                        () = async {
                            cancel.cancelled().await;
                            chat_cancel.cancel();
                        } => return,
                        result = run.write(input, &deps, &view, &chat_cancel) => result,
                        () = mark_ended(&mut status, meeting, &view) => unreachable!("the status watcher never completes"),
                    };
                    run.finish(job, begun.elapsed(), fingerprint, result, &view);
                    continue;
                }
                if ending && !run.scheduler.final_pending(&poll) {
                    return;
                }
            }
        }

        tokio::select! {
            changed = status.changed() => if changed.is_err() { return },
            () = tokio::time::sleep(deps.cadence.look_every), if !run.unavailable => {}
            () = cancel.cancelled() => return,
        }
    }
}

struct Run {
    scheduler: Scheduler,
    notes: Notes,
    seeded_coverage: Option<u64>,
    last_hint: Option<u64>,
    batch: Option<MemoBatch>,
    unavailable: bool,
}

impl Run {
    fn new(seed: Option<Notes>, cadence: Cadence) -> Self {
        let seeded_coverage = seed.as_ref().map(Notes::covered_ms);
        let notes = seed.unwrap_or_default();
        let scheduler = Scheduler::new(
            cadence.hint_every,
            cadence.final_gap,
            notes.now.is_some(),
            notes.memo.span_ms.map(|(_, until)| until),
        );
        Self { scheduler, notes, seeded_coverage, last_hint: None, batch: None, unavailable: false }
    }

    fn poll(
        &self,
        now: Duration,
        meeting_ms: u64,
        covered_ms: u64,
        active: Option<ActiveRecording>,
        has_text: bool,
        fingerprint: u64,
    ) -> Poll {
        let memo = &self.notes.memo;
        Poll {
            now,
            meeting_ms: meeting_ms.max(covered_ms),
            covered_ms,
            has_text,
            hint_changed: self.last_hint.map_or_else(
                || self.seeded_coverage.is_none_or(|seeded| covered_ms > seeded),
                |last| fingerprint != last,
            ),
            memo_changed: has_text && (memo.span_ms.is_none() || covered_ms > memo.covered_ms)
                || self.batch.as_ref().is_some_and(|batch| batch.last || active.is_some()),
            paused: active.is_some_and(|a| a.phase == RecordingPhase::Paused),
            ending: active.is_none(),
        }
    }

    fn start(&mut self, job: Job, poll: &Poll, context: &MeetingContext) {
        self.scheduler.started(job, poll);
        if let Job::Memo { last } = job
            && self.batch.as_ref().is_none_or(|batch| batch.last != last)
        {
            self.batch = Some(MemoBatch::new(context, &self.notes.memo, last));
        }
    }

    fn finish(
        &mut self,
        job: Job,
        now: Duration,
        fingerprint: u64,
        result: Result<(), Failure>,
        view: &watch::Sender<NotesView>,
    ) {
        let (outcome, phase, notes) = match result {
            Ok(()) => {
                if job == Job::Hint {
                    self.last_hint = Some(fingerprint);
                }
                (Outcome::Succeeded { more: self.batch.is_some() }, NotesPhase::Ready, Some(self.notes.clone()))
            }
            Err(error) => {
                match job {
                    Job::Hint => tracing::warn!(reason = %error, "meeting notes hint failed"),
                    Job::Memo { .. } => {
                        let chunk = self.batch.take().map_or(1, |batch| batch.written + 1);
                        tracing::warn!(chunk, reason = %error, "meeting notes memo failed");
                    }
                }
                let phase = match error {
                    Failure::Unavailable(why) => {
                        self.unavailable = true;
                        NotesPhase::Unavailable(why)
                    }
                    _ => NotesPhase::Ready,
                };
                (Outcome::Failed, phase, None)
            }
        };
        self.scheduler.finished(job, now, outcome);
        publish(view, |v| {
            v.phase = phase;
            if notes.is_some() {
                v.notes = notes;
            }
            v.stale = Stale { hint: self.scheduler.stale_hint(), memo: self.scheduler.stale_memo() };
        });
    }
}

async fn mark_ended(status: &mut watch::Receiver<TranscriptionStatus>, meeting: i64, view: &watch::Sender<NotesView>) {
    if !view.borrow().ended && status.wait_for(|s| s.state.transcript_id() != Some(meeting)).await.is_ok() {
        publish(view, |v| v.ended = true);
    }
    std::future::pending().await
}

fn hint_fingerprint(window: &[&Utterance]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for u in window {
        (u.speaker, u.start_ms, &u.text).hash(&mut hasher);
    }
    hasher.finish()
}

#[derive(Debug, thiserror::Error)]
enum Failure {
    #[error("model could not start: {0:?}")]
    Unavailable(Unavailable),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error("reply rejected: {0:?}")]
    Reply(ReplyError),
    #[error("reply dropped all memo entries")]
    EmptyMemo,
    #[error("request timed out after {0:?}")]
    Timeout(Duration),
}

impl From<ReplyError> for Failure {
    fn from(error: ReplyError) -> Self {
        Self::Reply(error)
    }
}

struct MemoBatch {
    utterances: Vec<Utterance>,
    consumed: usize,
    last: bool,
    written: usize,
}

impl MemoBatch {
    fn new(context: &MeetingContext, memo: &Memo, last: bool) -> Self {
        let mut pending: Vec<_> = context
            .utterances
            .iter()
            .filter(|u| memo.span_ms.is_none() || u.end_ms > memo.covered_ms)
            .cloned()
            .collect();
        pending.sort_by_key(|u| u.end_ms);
        Self { utterances: pending, consumed: 0, last, written: 0 }
    }

    fn chunk(&self, max_chars: usize) -> Vec<&Utterance> {
        let mut chunk = Vec::new();
        let mut chars = 0;
        // A watermark may advance past a timestamp only when all utterances ending there were sent.
        for group in self.utterances[self.consumed..].chunk_by(|a, b| a.end_ms == b.end_ms) {
            let len: usize = group.iter().map(|u| u.text.chars().count()).sum();
            if !chunk.is_empty() && chars + len > max_chars {
                break;
            }
            chunk.extend(group);
            chars += len;
        }
        chunk.sort_by_key(|u| u.start_ms);
        chunk
    }
}

struct JobInput<'a> {
    job: Job,
    context: &'a MeetingContext,
    window: &'a [&'a Utterance],
}

impl Run {
    async fn write(
        &mut self,
        input: JobInput<'_>,
        deps: &NotesDeps,
        view: &watch::Sender<NotesView>,
        cancel: &ChatCancel,
    ) -> Result<(), Failure> {
        let JobInput { job, context, window } = input;
        let ready = deps.source.model().await.map_err(Failure::Unavailable)?;
        if matches!(view.borrow().phase, NotesPhase::Working { loading_model: true }) {
            publish(view, |v| v.phase = NotesPhase::Working { loading_model: false });
        }
        let budget = Budget { thinking_off: ready.thinking_off, configured: ready.max_tokens };
        let memo_chunk;
        let (judge, utterances) = match job {
            Job::Hint => (Judge::Hint, window),
            Job::Memo { .. } => {
                let max_chars = (ready.limits.context_chars / 2).clamp(2_000, MEMO_CHUNK_CHARS);
                memo_chunk = self.batch.as_ref().expect("a memo job has a batch").chunk(max_chars);
                (Judge::Memo, memo_chunk.as_slice())
            }
        };
        let notes = &mut self.notes;
        let lang = meeting_lang(utterances, notes.lang, deps.prior, judge);
        let frame = Frame {
            context,
            lang: lang.request,
            ui_names: SpeakerNames { me: &deps.me, others: &deps.others },
            offset: deps.offset_secs,
        };
        let request = match job {
            Job::Hint => hint_request(&frame, utterances, notes.now.as_ref(), budget),
            Job::Memo { .. } => memo_request(&frame, utterances, &notes.memo, budget),
        };
        let reply = ask(&ready, &request, request_timeout(job, ready.local), cancel).await?;
        match job {
            Job::Hint => {
                let topic = read_hint(&reply)?;
                add_hint(notes, Hint { topic, covered_ms: context.covered_ms() });
            }
            Job::Memo { .. } => {
                let memo = read_memo(&reply)?;
                if memo.is_empty() && !notes.memo.is_empty() {
                    return Err(Failure::EmptyMemo);
                }
                let (from, until) = utterances
                    .iter()
                    .fold((u64::MAX, 0), |(from, until), u| (from.min(u.start_ms), until.max(u.end_ms)));
                apply_memo(notes, memo, from, until);

                let written = utterances.len();
                let batch = self.batch.as_mut().expect("a memo job has a batch");
                batch.consumed += written;
                batch.written += 1;
                if batch.consumed == batch.utterances.len() {
                    self.batch = None;
                }
            }
        }
        notes.started_ts = context.started_ts;
        notes.lang = lang.settled;
        Ok(())
    }
}

fn apply_memo(notes: &mut Notes, memo: Memo, from: u64, until: u64) {
    let from = notes.memo.span_ms.map_or(from, |(start, _)| start.min(from));
    notes.memo = Memo { span_ms: Some((from, until)), covered_ms: until, ..memo };
    notes.trail.retain(|hint| hint.covered_ms > until);
    if notes.now.as_ref().is_some_and(|hint| hint.covered_ms <= until) {
        notes.now = None;
    }
}

fn add_hint(notes: &mut Notes, hint: Hint) {
    let previous = notes.now.replace(hint);
    let Some(previous) = previous else { return };
    if notes.memo.span_ms.is_some() && previous.covered_ms <= notes.memo.covered_ms
        || notes.now.as_ref().is_some_and(|current| current.topic == previous.topic)
    {
        return;
    }
    if notes.trail.last().is_some_and(|last| last.topic == previous.topic) {
        notes.trail.pop();
    }
    notes.trail.push(previous);
    if notes.trail.len() > MAX_TRAIL {
        notes.trail.remove(0);
    }
}

fn request_timeout(job: Job, local: bool) -> Duration {
    Duration::from_secs(match (job, local) {
        (Job::Hint, false) => 20,
        (Job::Hint, true) => 45,
        (Job::Memo { .. }, false) => 60,
        (Job::Memo { .. }, true) => 180,
    })
}

async fn ask(
    ready: &NotesModel,
    request: &ChatRequest,
    timeout: Duration,
    cancel: &ChatCancel,
) -> Result<String, Failure> {
    let mut ignore = |_: &str| {};
    tokio::select! {
        biased;
        reply = ready.model.chat(request, &mut ignore, cancel) => Ok(reply?),
        () = async {
            tokio::time::sleep(timeout).await;
            cancel.cancel();
        } => Err(Failure::Timeout(timeout)),
    }
}

#[cfg(test)]
mod tests;
