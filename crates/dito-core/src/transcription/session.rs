//! The actor that owns the running transcription session.
//!
//! Data flows one way. Source threads cut each stream into utterances ([`super::source`]); one recognizer task turns
//! their audio into text ([`super::recognizer`]); this loop owns everything shown while recording (the open
//! paragraph and the words still being heard) and every paragraph it closes, and publishes both together: a closed
//! paragraph is stored before the status update that drops it from [`TranscriptionStatus::live`], and that update
//! also bumps the revision and counts it in [`TranscriptionStatus::closed`]. The store can run ahead of the status
//! a view last received, so a view shows only the first `closed` closed paragraphs of the transcript: each word is then
//! in exactly one place.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use super::live::{Hearing, LiveText};
use super::paragraphs::{self, Paragraph, Paragrapher};
use super::prompt::{CONTEXT_PARAGRAPHS, ContextParagraph, build_prompt, worth_polishing};
use super::recognizer::{Job, Recognized, Recognizer, Utterance};
use super::source::{SourceEvent, detect};
use super::wordbook::SessionWordbook;
use super::{AudioSource, LiveAudio, LiveInput, Sources, Speaker, VoiceDetectors};
use crate::config::ConfigStore;
use crate::factory::Engines;
use crate::i18n::{Lang, Tr};
use crate::prompts::fence;
use crate::provider::{TextModel, TextRequest};
use crate::store::Store;

const IDLE_TICK: Duration = Duration::from_millis(250);

/// Each audio source of a session and the speaker it records.
const SOURCES: [(AudioSource, Speaker); 2] =
    [(AudioSource::System, Speaker::Others), (AudioSource::Microphone, Speaker::Me)];

pub struct TranscriptionDeps {
    pub audio: Arc<dyn LiveAudio>,
    pub detectors: Arc<dyn VoiceDetectors>,
    pub engines: Arc<dyn Engines>,
    pub config: Arc<ConfigStore>,
    pub store: Arc<Store>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum SessionState {
    #[default]
    Idle,
    /// Loading engines and opening the audio.
    Starting,
    /// Recording, or paused mid-recording. `started_ts` stays when the session began, for storage. The time
    /// recorded so far is `recorded_ms` plus the stretch that began at the unix second `segment_start`, which is
    /// `None` while paused: the elapsed time ticks while recording and freezes while paused ([`Self::elapsed_ms`]).
    Recording { id: i64, started_ts: i64, sources: Sources, recorded_ms: u64, segment_start: Option<i64> },
    /// The audio is closed; the last utterances are being recognized and polished.
    Finishing { id: i64 },
}

impl SessionState {
    /// The milliseconds recorded so far, computed from the status alone: the closed stretches plus the one open
    /// now. Ticks while recording, freezes while paused, and is zero in every other state.
    pub fn elapsed_ms(&self) -> u64 {
        match self {
            Self::Recording { recorded_ms, segment_start, .. } => {
                recorded_ms + segment_start.map_or(0, |start| crate::unix_seconds().saturating_sub(start) as u64 * 1000)
            }
            Self::Idle | Self::Starting | Self::Finishing { .. } => 0,
        }
    }

    pub fn transcript_id(&self) -> Option<i64> {
        match self {
            Self::Recording { id, .. } | Self::Finishing { id } => Some(*id),
            Self::Idle | Self::Starting => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TranscriptionStatus {
    pub state: SessionState,
    /// The last failure, kept until the next start.
    pub error: Option<String>,
    /// Bumped whenever a stored transcript changes, so views reload.
    pub revision: u64,
    /// The paragraph still being heard, per speaker. Never read from the store: stored paragraphs that are still
    /// listening exist only so a crash keeps their words.
    pub live: Vec<LiveText>,
    /// How many paragraphs of the running session's transcript have closed, as of `live`, counting those a resumed
    /// transcript already held. Paragraphs close in the order they started.
    pub closed: usize,
}

enum Cmd {
    /// `resume` names a finished transcript the session appends to instead of starting a new one.
    Start {
        sources: Sources,
        resume: Option<i64>,
    },
    SetSources {
        sources: Sources,
    },
    TogglePause,
    Stop,
}

#[derive(Clone)]
pub struct TranscriptionHandle {
    tx: mpsc::UnboundedSender<Cmd>,
    status: watch::Receiver<TranscriptionStatus>,
}

impl TranscriptionHandle {
    /// Ignored unless idle.
    pub fn start(&self, sources: Sources) {
        let _ = self.tx.send(Cmd::Start { sources, resume: None });
    }

    /// Records another session into the finished transcript `id`: its paragraphs follow the earlier ones on the
    /// meeting's own clock. Ignored unless idle; a transcript that is gone or still recording fails the start.
    pub fn resume(&self, id: i64, sources: Sources) {
        let _ = self.tx.send(Cmd::Start { sources, resume: Some(id) });
    }

    /// The session finishes the utterances it already heard before it ends.
    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }

    /// Turns sources on or off in the running session. Ignored unless recording, and when neither source is set.
    pub fn set_sources(&self, sources: Sources) {
        let _ = self.tx.send(Cmd::SetSources { sources });
    }

    /// Pauses the running session, or resumes it when paused. Ignored unless recording.
    pub fn toggle_pause(&self) {
        let _ = self.tx.send(Cmd::TogglePause);
    }

    pub fn status(&self) -> TranscriptionStatus {
        self.status.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<TranscriptionStatus> {
        self.status.clone()
    }
}

/// Needs a Tokio runtime. Sessions cut short by an earlier quit are closed first.
pub fn spawn(deps: TranscriptionDeps) -> TranscriptionHandle {
    if let Err(e) = deps.store.recover_transcripts(crate::unix_seconds()) {
        tracing::warn!("could not close unfinished transcripts: {e}");
    }
    let (tx, rx) = mpsc::unbounded_channel();
    let (status_tx, status_rx) = watch::channel(TranscriptionStatus::default());
    tokio::spawn(actor(Arc::new(deps), rx, Arc::new(status_tx)));
    TranscriptionHandle { tx, status: status_rx }
}

type Status = Arc<watch::Sender<TranscriptionStatus>>;

/// The running session: how to end it, how to tell it the sources changed, and the flag its open audio streams
/// read on every chunk so a paused session hears nothing.
struct Running {
    stop: CancellationToken,
    sources: watch::Sender<Sources>,
    paused: Arc<AtomicBool>,
}

async fn actor(deps: Arc<TranscriptionDeps>, mut rx: mpsc::UnboundedReceiver<Cmd>, status: Status) {
    let mut running: Option<Running> = None;
    while let Some(cmd) = rx.recv().await {
        match cmd {
            Cmd::Start { sources, resume } => {
                if status.borrow().state != SessionState::Idle {
                    continue;
                }
                let stop = CancellationToken::new();
                let (source_tx, source_rx) = watch::channel(sources);
                let paused = Arc::new(AtomicBool::new(false));
                running = Some(Running { stop: stop.clone(), sources: source_tx, paused: paused.clone() });
                status.send_modify(|s| {
                    s.state = SessionState::Starting;
                    s.error = None;
                });
                tokio::spawn(run(deps.clone(), sources, resume, stop, status.clone(), source_rx, paused));
            }
            Cmd::SetSources { sources } => {
                if let Some(running) = running.as_ref()
                    && (sources.system || sources.microphone)
                {
                    let _ = running.sources.send(sources);
                }
            }
            Cmd::TogglePause => {
                let Some(running) = running.as_ref() else { continue };
                let recording = matches!(status.borrow().state, SessionState::Recording { segment_start: Some(_), .. });
                set_paused(&status, &running.paused, recording);
            }
            Cmd::Stop => {
                if let Some(running) = running.take() {
                    running.stop.cancel();
                }
            }
        }
    }
}

/// The pause closes the open stretch of recording into `recorded_ms`; the resume opens a new one. Nothing happens
/// when the session is not recording or is already in that state.
fn set_paused(status: &Status, paused: &AtomicBool, pause: bool) {
    let now = crate::unix_seconds();
    let fold_ms = {
        let s = status.borrow();
        let SessionState::Recording { segment_start, .. } = &s.state else { return };
        match (*segment_start, pause) {
            (Some(start), true) => now.saturating_sub(start) as u64 * 1000,
            (None, false) => 0,
            _ => return,
        }
    };
    // Set before the status tells the world: once a view sees the pause, no stream delivers audio.
    paused.store(pause, Ordering::Relaxed);
    status.send_modify(|s| {
        let SessionState::Recording { recorded_ms, segment_start, .. } = &mut s.state else { return };
        if pause {
            *recorded_ms += fold_ms;
            *segment_start = None;
        } else {
            *segment_start = Some(now);
        }
    });
}

const NO_SOURCES: Sources = Sources { system: false, microphone: false };

/// What reconciling the open streams with the wanted sources needs from the session.
struct Streams<'a> {
    deps: &'a TranscriptionDeps,
    session_start: Instant,
    paused: &'a Arc<AtomicBool>,
    status: &'a Status,
    lang: Lang,
}

impl Streams<'_> {
    /// The one path that brings the open streams in line with `wanted`, after a toggle and on every tick. It closes
    /// what is no longer wanted, drops a stream that stopped on its own (`design:tools/recording`, 23.02.5) and opens
    /// every wanted source that has no stream, each once; a paused session hears nothing from it until the resume. A
    /// source that was lost or would not open is turned off while another stream records. With nothing left open,
    /// the session pauses, keeps `wanted` as the toggles show it, and holds those sources back until the resume
    /// instead of retrying them on every tick. Returns the open sources.
    fn reconcile(
        &self,
        open: &mut Vec<(AudioSource, Box<dyn LiveInput>)>,
        wanted: &mut Sources,
        held_back: &mut Sources,
        heard: &mpsc::UnboundedSender<SourceEvent>,
        taps: &mut Vec<Source>,
        generations: &mut Generations,
    ) -> Sources {
        let paused = self.paused.load(Ordering::Relaxed);
        if !paused {
            *held_back = NO_SOURCES;
        }
        open.retain(|(source, _)| on(*wanted, *source));
        let mut dropped = Vec::new();
        open.retain(|(source, input)| {
            let Some(why) = input.lost() else { return true };
            tracing::warn!("transcription: the {source:?} stream stopped: {why}");
            let message = match source {
                AudioSource::Microphone => Tr::TranscriptionMicrophoneLost,
                AudioSource::System => Tr::TranscriptionSystemAudioLost,
            };
            self.status.send_modify(|s| s.error = Some(self.lang.tr(message).to_owned()));
            dropped.push(*source);
            false
        });
        for (source, speaker) in SOURCES {
            let skip = !on(*wanted, source)
                || (paused && on(*held_back, source))
                || dropped.contains(&source)
                || open.iter().any(|(s, _)| *s == source);
            if skip {
                continue;
            }
            let generation = generations.next(source);
            match open_source(self.deps, source, speaker, self.session_start, heard, generation, self.paused) {
                Ok((input, tap)) => {
                    open.push((source, input));
                    taps.push(tap);
                }
                Err(e) => {
                    self.status.send_modify(|s| s.error = Some(e));
                    dropped.push(source);
                }
            }
        }
        let nothing_open = open.is_empty();
        for source in dropped {
            let (set, value) = if nothing_open { (&mut *held_back, true) } else { (&mut *wanted, false) };
            match source {
                AudioSource::System => set.system = value,
                AudioSource::Microphone => set.microphone = value,
            }
        }
        if nothing_open && *held_back != NO_SOURCES {
            set_paused(self.status, self.paused, true);
        }
        let shown = *wanted;
        self.status.send_if_modified(|s| match &mut s.state {
            SessionState::Recording { sources, .. } if *sources != shown => {
                *sources = shown;
                true
            }
            _ => false,
        });
        open_sources(open)
    }
}

fn open_sources(open: &[(AudioSource, Box<dyn LiveInput>)]) -> Sources {
    Sources {
        system: open.iter().any(|(s, _)| *s == AudioSource::System),
        microphone: open.iter().any(|(s, _)| *s == AudioSource::Microphone),
    }
}

async fn run(
    deps: Arc<TranscriptionDeps>,
    sources: Sources,
    resume: Option<i64>,
    stop: CancellationToken,
    status: Status,
    source_rx: watch::Receiver<Sources>,
    paused: Arc<AtomicBool>,
) {
    let result = record(&deps, sources, resume, &stop, &status, source_rx, &paused).await;
    if let Err(e) = &result {
        tracing::warn!("transcription failed: {e}");
    }
    status.send_modify(|s| {
        s.state = SessionState::Idle;
        s.revision += 1;
        s.live.clear();
        if let Err(e) = result {
            s.error = Some(e);
        }
    });
}

/// A paragraph that waits for its polish. `lang` is the language the engine reported for it, which its spoken forms
/// are matched in again.
struct Closed {
    id: i64,
    speaker: Speaker,
    text: String,
    lang: Option<Lang>,
}

struct Source {
    speaker: Speaker,
    speaking: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<()>,
}

/// How often each source has been opened in the session, in [`SOURCES`] order. It numbers the utterances of the
/// source, so a reopened source never reuses or goes below a number it already used.
#[derive(Default)]
struct Generations([u64; 2]);

impl Generations {
    fn next(&mut self, source: AudioSource) -> u64 {
        let i = match source {
            AudioSource::System => 0,
            AudioSource::Microphone => 1,
        };
        let generation = self.0[i];
        self.0[i] += 1;
        generation
    }
}

fn on(sources: Sources, source: AudioSource) -> bool {
    match source {
        AudioSource::System => sources.system,
        AudioSource::Microphone => sources.microphone,
    }
}

/// Opens one source's stream and starts the thread that cuts it into utterances.
fn open_source(
    deps: &TranscriptionDeps,
    source: AudioSource,
    speaker: Speaker,
    session_start: Instant,
    heard: &mpsc::UnboundedSender<SourceEvent>,
    generation: u64,
    paused: &Arc<AtomicBool>,
) -> Result<(Box<dyn LiveInput>, Source), String> {
    let detector = deps.detectors.create()?;
    // Stamped on the audio thread: the detector thread may fall behind, and its clock would then see gaps
    // that are only a backlog.
    let (chunk_tx, chunk_rx) = std_mpsc::channel::<(Instant, Vec<f32>)>();
    let gate = paused.clone();
    let input = deps
        .audio
        .open(
            source,
            Box::new(move |chunk| {
                // A paused session hears nothing: the frame is dropped before it reaches the detector.
                if gate.load(Ordering::Relaxed) {
                    return;
                }
                let _ = chunk_tx.send((Instant::now(), chunk.to_vec()));
            }),
        )
        .map_err(|e| e.to_string())?;
    let speaking = Arc::new(AtomicBool::new(false));
    let (flag, tx) = (speaking.clone(), heard.clone());
    let thread = std::thread::Builder::new()
        .name("dito-transcription-vad".into())
        .spawn(move || detect(detector, chunk_rx, speaker, session_start, generation << 32, &flag, &tx))
        .map_err(|e| e.to_string())?;
    Ok((input, Source { speaker, speaking, thread }))
}

async fn record(
    deps: &TranscriptionDeps,
    sources: Sources,
    resume: Option<i64>,
    stop: &CancellationToken,
    status: &Status,
    mut source_rx: watch::Receiver<Sources>,
    paused: &Arc<AtomicBool>,
) -> Result<(), String> {
    let cfg = deps.config.get();
    let asr = deps.engines.transcriber(&cfg).await.map_err(|e| e.to_string())?;
    let polisher = match deps.engines.text_model(&cfg).await {
        Ok(p) => p.map(Ok),
        Err(e) => Some(Err(e.to_string())),
    };
    let titler = polisher.as_ref().and_then(|p| p.as_ref().ok().cloned()).filter(|_| cfg.transcription.auto_title);
    let wordbook = Arc::new(SessionWordbook::load(deps.store.clone(), cfg.wordbook_for_polish));
    let mut wanted = Vec::new();
    for (source, speaker) in SOURCES {
        if on(sources, source) {
            wanted.push((source, speaker));
        }
    }
    if wanted.is_empty() {
        return Err("no audio source selected".into());
    }
    if stop.is_cancelled() {
        return Ok(());
    }

    let session_start = Instant::now();
    let (heard_tx, mut heard_rx) = mpsc::unbounded_channel::<SourceEvent>();
    let mut generations = Generations::default();
    let mut opened = Vec::new();
    let mut taps = Vec::new();
    for (source, speaker) in wanted {
        let generation = generations.next(source);
        let (input, tap) = open_source(deps, source, speaker, session_start, &heard_tx, generation, paused)?;
        opened.push((source, input));
        taps.push(tap);
    }
    // Kept open until the session stops: the threads alone would close `heard_rx` before the user is done.
    let mut heard = Some(heard_tx);

    let session_ts = crate::unix_seconds();
    // A resumed transcript keeps its start, so the new paragraphs are timed from it and land after the old ones.
    let (id, started_ts, held, earlier) = match resume {
        Some(id) => {
            let reopened = deps
                .store
                .reopen_transcript(id, sources)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("transcript {id} cannot be resumed"))?;
            (id, reopened.started_ts, reopened.sources, reopened.paragraphs)
        }
        None => (deps.store.create_transcript(session_ts, sources).map_err(|e| e.to_string())?, session_ts, sources, 0),
    };
    let offset_ms = (session_ts - started_ts).max(0) as u64 * 1000;
    status.send_modify(|s| {
        s.state = SessionState::Recording { id, started_ts, sources, recorded_ms: 0, segment_start: Some(session_ts) };
        s.revision += 1;
        s.live.clear();
        s.closed = earlier;
    });

    let (job_tx, job_rx) = mpsc::unbounded_channel::<Job>();
    let (text_tx, mut text_rx) = mpsc::unbounded_channel::<Recognized>();
    let recognizer = Recognizer {
        asr,
        language: cfg.general.preferred_language.code().map(str::to_string),
        wordbook: wordbook.clone(),
    };
    let recognizer = tokio::spawn(recognizer.run(job_rx, text_tx));
    let (polish_tx, polish_rx) = mpsc::unbounded_channel::<Closed>();
    // Read per paragraph: the user may turn the second source on or off while the session runs.
    let labelled = Arc::new(AtomicBool::new(held.both()));
    let polish_task =
        tokio::spawn(polish_loop(deps.store.clone(), polisher, wordbook, labelled.clone(), polish_rx, status.clone()));
    let mut session = Session {
        store: &deps.store,
        id,
        offset_ms,
        status,
        polish_tx,
        paragraphs: Paragrapher::default(),
        open_id: None,
        hearing: Hearing::default(),
        published: Vec::new(),
        closed: earlier,
    };
    let mut inputs = Some(opened);
    // The sources the user wants, which the toggles show; `reconcile` brings the open streams in line with it.
    let mut wanted = sources;
    // Lost with nothing else open: opened again by the resume, not retried while paused.
    let mut held_back = NO_SOURCES;
    // Every source the transcript holds, kept in the store so it shows what it holds.
    let mut union = held;
    // `None` once every source has ended: the recognizer then finishes what it has and closes `text_rx`.
    let mut jobs = Some(job_tx);
    let mut sources_open = true;
    let mut tick = tokio::time::interval(IDLE_TICK);
    let mut stopped = None;
    let streams = Streams { deps, session_start, paused, status, lang: cfg.general.lang() };
    let note_sources = |union: &mut Sources, now: Sources| {
        let ever = Sources { system: union.system || now.system, microphone: union.microphone || now.microphone };
        if ever != *union {
            *union = ever;
            labelled.store(union.both(), Ordering::Relaxed);
            if let Err(e) = deps.store.set_transcript_sources(id, *union) {
                tracing::warn!("could not store the sources of the transcript: {e}");
            }
        }
    };
    loop {
        tokio::select! {
            _ = stop.cancelled(), if inputs.is_some() => {
                // Dropping the inputs ends the streams; the source threads then flush what they hold. Dropping the
                // sender here is what lets `heard_rx` close once every one of them has ended.
                inputs = None;
                heard = None;
                // Taken before Finishing zeroes it: the recognition, polishing and naming that follow are not recording.
                stopped = Some((status.borrow().state.elapsed_ms(), crate::unix_seconds()));
                status.send_modify(|s| s.state = SessionState::Finishing { id });
            }
            changed = source_rx.changed(), if inputs.is_some() => {
                // Only the actor's own sender could close it, and it lives while this session does.
                if changed.is_err() {
                    continue;
                }
                wanted = *source_rx.borrow_and_update();
                let (Some(open), Some(heard)) = (inputs.as_mut(), heard.as_ref()) else { continue };
                let now = streams.reconcile(open, &mut wanted, &mut held_back, heard, &mut taps, &mut generations);
                note_sources(&mut union, now);
            }
            event = heard_rx.recv(), if sources_open => {
                let Some(event) = event else {
                    sources_open = false;
                    jobs = None;
                    continue;
                };
                let mut batch = vec![event];
                while let Ok(event) = heard_rx.try_recv() {
                    batch.push(event);
                }
                let mut changed = false;
                for event in batch {
                    changed |= session.heard(event, jobs.as_ref(), inputs.is_some());
                }
                if changed {
                    session.publish(false);
                }
            }
            text = text_rx.recv() => {
                let Some(text) = text else { break };
                let mut batch = vec![text];
                while let Ok(text) = text_rx.try_recv() {
                    batch.push(text);
                }
                let mut stored = false;
                for text in batch {
                    stored |= session.recognized(text);
                }
                session.publish(stored);
            }
            _ = tick.tick() => {
                if let (Some(open), Some(heard)) = (inputs.as_mut(), heard.as_ref()) {
                    let now = streams.reconcile(open, &mut wanted, &mut held_back, heard, &mut taps, &mut generations);
                    note_sources(&mut union, now);
                }
                let now_ms = session_start.elapsed().as_millis() as u64;
                let quiet = |speaker: Speaker| {
                    !session.hearing.busy(speaker)
                        && taps.iter().filter(|t| t.speaker == speaker).all(|t| !t.speaking.load(Ordering::Relaxed))
                };
                let idle = session.paragraphs.open().is_some_and(|p| quiet(p.speaker));
                if idle && let Some(p) = session.paragraphs.close_if_idle(now_ms) {
                    session.close(p);
                    session.publish(true);
                }
            }
        }
    }
    let _ = recognizer.await;
    if let Some(p) = session.paragraphs.take() {
        session.close(p);
    }
    session.publish(true);
    drop(session);
    let _ = polish_task.await;
    if let Some(model) = titler {
        let system = title_system_prompt(cfg.transcription.title_instruction(), cfg.general.lang());
        name_untitled(&deps.store, model.as_ref(), &system, id, status).await;
    }
    for source in taps {
        let _ = tokio::task::spawn_blocking(move || source.thread.join()).await;
    }
    let (recorded_ms, ended_ts) =
        stopped.unwrap_or_else(|| (status.borrow().state.elapsed_ms(), crate::unix_seconds()));
    deps.store.end_transcript(id, recorded_ms, ended_ts).map_err(|e| e.to_string())?;
    Ok(())
}

struct Session<'a> {
    store: &'a Store,
    id: i64,
    /// Where this session's clock starts on the transcript's: zero for a new transcript, the time since the meeting
    /// began for a resumed one. Stored and published times carry it; the session's own bookkeeping does not.
    offset_ms: u64,
    status: &'a Status,
    polish_tx: mpsc::UnboundedSender<Closed>,
    paragraphs: Paragrapher,
    /// The stored row of the open paragraph.
    open_id: Option<i64>,
    hearing: Hearing,
    /// The live text last published, so an update that changes nothing is not sent.
    published: Vec<LiveText>,
    closed: usize,
}

impl Session<'_> {
    /// Returns whether the words still being heard changed. Partial audio is passed on only while recording.
    fn heard(&mut self, event: SourceEvent, jobs: Option<&mpsc::UnboundedSender<Job>>, recording: bool) -> bool {
        match event {
            SourceEvent::Partial { speaker, seq, start_ms, samples } => {
                if recording && let Some(jobs) = jobs {
                    let _ = jobs.send(Job::Partial { speaker, seq, start_ms, samples });
                }
                false
            }
            SourceEvent::Ended { speaker, seq, start_ms, end_ms, samples } => {
                self.hearing.ended(speaker, seq, start_ms);
                if let Some(jobs) = jobs {
                    let _ = jobs.send(Job::Final(Utterance { speaker, seq, start_ms, end_ms, samples }));
                }
                false
            }
            SourceEvent::Dropped { speaker, seq } => self.hearing.dropped(speaker, seq),
        }
    }

    /// Returns whether a stored paragraph changed.
    fn recognized(&mut self, text: Recognized) -> bool {
        match text {
            Recognized::Partial { speaker, seq, start_ms, text } => {
                self.hearing.partial(speaker, seq, start_ms, text);
                false
            }
            Recognized::Final { speaker, seq, start_ms, end_ms, text, lang } => {
                let start_ms = self.hearing.start_of(speaker, seq).map_or(start_ms, |s| s.min(start_ms));
                self.hearing.finished_utterance(speaker, seq);
                let Some(text) = text else { return false };
                for p in self.paragraphs.push(paragraphs::Utterance { speaker, start_ms, end_ms, text, lang }) {
                    self.close(p);
                }
                if let Some(p) = self.paragraphs.open().cloned() {
                    self.store_open(&p);
                }
                true
            }
            Recognized::Failed(e) => {
                self.status.send_modify(|s| s.error = Some(e));
                false
            }
        }
    }

    /// One status update carries the live text and, when `stored`, the revision that makes views re-read the store.
    fn publish(&mut self, stored: bool) {
        let mut live = self.hearing.view(self.paragraphs.open());
        for text in &mut live {
            text.start_ms += self.offset_ms;
        }
        if !stored && live == self.published {
            return;
        }
        self.published = live.clone();
        let closed = self.closed;
        self.status.send_modify(|s| {
            s.live = live;
            s.closed = closed;
            if stored {
                s.revision += 1;
            }
        });
    }

    /// Writes the paragraph to the open row, or to a new row when none is open.
    fn store_open(&mut self, p: &Paragraph) {
        let (start_ms, end_ms) = (p.start_ms + self.offset_ms, p.end_ms + self.offset_ms);
        let stored = match self.open_id {
            Some(id) => self.store.extend_paragraph(id, start_ms, end_ms, &p.text).map(|()| id),
            None => self.store.add_paragraph(self.id, p.speaker, start_ms, end_ms, &p.text),
        };
        self.open_id = stored.inspect_err(|e| tracing::warn!("could not store a paragraph: {e}")).ok();
    }

    /// The first paragraph a push closes is the one already stored as open; later ones get rows of their own. A closed
    /// paragraph reads as polishing at once, even while it waits its turn.
    fn close(&mut self, p: Paragraph) {
        self.store_open(&p);
        if let Some(id) = self.open_id.take() {
            if let Err(e) = self.store.set_paragraph_polishing(id) {
                tracing::warn!("could not store a paragraph: {e}");
            }
            let _ = self.polish_tx.send(Closed { id, speaker: p.speaker, text: p.text, lang: p.lang });
            self.closed += 1;
        }
    }
}

/// A generated title reads only the beginning of a long meeting, where its subject is usually named.
const TITLE_INPUT_CHARS: usize = 3_000;
const TITLE_MAX_CHARS: usize = 40;
/// Naming never holds the end of a session for long.
const TITLE_TIMEOUT: Duration = Duration::from_secs(15);

fn title_system_prompt(instruction: &str, ui_lang: Lang) -> String {
    let name = ui_lang.name(Lang::En);
    let code = ui_lang.code();
    format!(
        "Write a title for the meeting transcript enclosed between the === lines. \
         Infer the main language spoken in the meeting from the transcript and use that language for the title. \
         The app interface language is {name} ({code}) and is provided as context only. \
         Use the title instructions for style and format while keeping the meeting's language. \
         Reply with the title only, on one line, without quotes or a final period.\n\n\
         Title instructions\n{instruction}"
    )
}

/// Names a meeting the user left untitled. Any failure leaves it untitled, and a title the user typed while the model
/// was writing is never replaced.
async fn name_untitled(store: &Store, model: &dyn TextModel, system: &str, id: i64, status: &Status) {
    let untitled = || matches!(store.get_transcript(id), Ok(Some(t)) if t.summary.title.trim().is_empty());
    let Ok(Some(transcript)) = store.get_transcript(id) else { return };
    if !transcript.summary.title.trim().is_empty() {
        return;
    }
    let input = title_input(&transcript.paragraphs);
    if input.is_empty() {
        return;
    }
    let user = fence(&input);
    let request = TextRequest { system, examples: &[], text: &user };
    let reply = match tokio::time::timeout(TITLE_TIMEOUT, model.polish(&request)).await {
        Ok(Ok(reply)) => reply,
        Ok(Err(e)) => return tracing::warn!("could not name transcript {id}: {e}"),
        Err(_) => return tracing::warn!("naming transcript {id} timed out"),
    };
    let Some(title) = clean_title(&reply) else { return };
    if !untitled() {
        return;
    }
    match store.set_transcript_title(id, &title) {
        Ok(()) => status.send_modify(|s| s.revision += 1),
        Err(e) => tracing::warn!("could not store the title of transcript {id}: {e}"),
    }
}

/// The meeting's words in order, polished where they are, cut at the input limit.
fn title_input(paragraphs: &[crate::store::TranscriptParagraph]) -> String {
    let mut input = String::new();
    for p in paragraphs {
        let words = if p.text.trim().is_empty() { p.raw.trim() } else { p.text.trim() };
        if words.is_empty() {
            continue;
        }
        if !input.is_empty() {
            input.push('\n');
        }
        input.push_str(words);
        if input.chars().count() >= TITLE_INPUT_CHARS {
            break;
        }
    }
    input.chars().take(TITLE_INPUT_CHARS).collect()
}

/// The first line of the reply without the quotes or brackets a model likes to add, capped; `None` when empty.
fn clean_title(reply: &str) -> Option<String> {
    let line = reply.lines().map(str::trim).find(|line| !line.is_empty())?;
    let quotes: &[char] = &['"', '\x27', '`', '\u{300C}', '\u{300D}', '\u{300E}', '\u{300F}', '\u{201C}', '\u{201D}'];
    let bare = line.trim_end_matches(['.', '\u{3002}']).trim_matches(quotes).trim_end_matches(['.', '\u{3002}']);
    let title: String = bare.trim().chars().take(TITLE_MAX_CHARS).collect();
    (!title.trim().is_empty()).then(|| title.trim().to_owned())
}

/// Polishes each paragraph as it closes and stores the result. The model may spell a spoken form out again, so the
/// polished text is restored with the wordbook's spoken forms in the language the engine reported for the paragraph.
async fn polish_loop(
    store: Arc<Store>,
    polisher: Option<Result<Arc<dyn TextModel>, String>>,
    wordbook: Arc<SessionWordbook>,
    labelled: Arc<AtomicBool>,
    mut rx: mpsc::UnboundedReceiver<Closed>,
    status: Status,
) {
    let mut context: VecDeque<ContextParagraph> = VecDeque::new();
    while let Some(p) = rx.recv().await {
        let started = Instant::now();
        let (text, error) = match &polisher {
            None => (p.text.clone(), None),
            _ if !worth_polishing(&p.text) => (p.text.clone(), None),
            Some(Err(e)) => (p.text.clone(), Some(e.clone())),
            Some(Ok(polisher)) => {
                let wordbook = wordbook.current();
                let labelled = labelled.load(Ordering::Relaxed);
                let prompt = build_prompt(&p.text, context.make_contiguous(), labelled, &wordbook.polish_terms);
                match polisher
                    .polish(&TextRequest { system: &prompt.system, examples: prompt.examples, text: &prompt.user })
                    .await
                {
                    Ok(out) if !out.trim().is_empty() => {
                        (wordbook.forms.apply_for_lang(out.trim(), p.lang).into_owned(), None)
                    }
                    Ok(_) => (p.text.clone(), None),
                    Err(e) => (p.text.clone(), Some(e.to_string())),
                }
            }
        };
        tracing::debug!("polished paragraph {} in {:?}", p.id, started.elapsed());
        if let Err(e) = store.finish_paragraph(p.id, &text, error.as_deref()) {
            tracing::warn!("could not store a polished paragraph: {e}");
        }
        status.send_modify(|s| s.revision += 1);
        context.push_back(ContextParagraph { speaker: p.speaker, text });
        if context.len() > CONTEXT_PARAGRAPHS {
            context.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock, Weak};

    use super::*;
    use crate::controller::RecorderError;
    use crate::factory::FixedEngines;
    use crate::provider::mock::{MockTextModel, MockTranscriber};
    use crate::store::{ParagraphState, Speaker};
    use crate::transcription::{ChunkSink, EnergyDetectors, LiveInput};

    #[tokio::test]
    async fn paragraph_editor_receives_speech_as_data_instead_of_a_request() {
        struct Editor;

        #[async_trait::async_trait]
        impl TextModel for Editor {
            async fn polish(&self, request: &TextRequest<'_>) -> crate::error::ProviderResult<String> {
                assert_eq!(request.text, "===\n检查一下这段话里面有没有英文音译词。\n===");
                assert_eq!(request.examples.len(), 3);
                assert!(request.examples.iter().all(|example| example.said.starts_with("===\n")));
                Ok("检查一下这段话里面有没有英文音译词。".into())
            }
        }

        let raw = "检查一下这段话里面有没有英文音译词。";
        let store = Arc::new(Store::open_in_memory().unwrap());
        let transcript = store.create_transcript(1, Sources::default()).unwrap();
        let id = store.add_paragraph(transcript, Speaker::Me, 0, 1_000, raw).unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(Closed { id, speaker: Speaker::Me, text: raw.into(), lang: None }).unwrap();
        drop(tx);
        let status = Arc::new(watch::channel(TranscriptionStatus::default()).0);
        let wordbook = Arc::new(SessionWordbook::load(store.clone(), false));
        polish_loop(store.clone(), Some(Ok(Arc::new(Editor))), wordbook, Arc::new(AtomicBool::new(false)), rx, status)
            .await;
        let result = store.get_transcript(transcript).unwrap().unwrap();
        assert_eq!(result.paragraphs[0].raw, raw);
        assert_eq!(result.paragraphs[0].text, raw);
        assert_eq!(result.paragraphs[0].state, ParagraphState::Done);
    }

    #[tokio::test]
    async fn a_polished_paragraph_has_its_spoken_forms_written_again_in_its_language() {
        // Answers with the paragraph it was given, as a model that spells the forms out again does.
        struct Echo;
        #[async_trait::async_trait]
        impl TextModel for Echo {
            async fn polish(&self, request: &TextRequest<'_>) -> crate::error::ProviderResult<String> {
                Ok(crate::prompts::unfenced(request.text).to_string())
            }
        }
        // The paragraph, the language the engine reported for it, and what is stored: a form scoped to Japanese
        // waits for Japanese to be reported, while a form for every language always applies.
        let paragraphs = [
            ("明日のmeetingです。", Some(Lang::Ja), "明日のMTGです。"),
            ("une pull request demain", None, "une PR demain"),
            ("On a un meeting demain", None, "On a un meeting demain"),
        ];
        let store = Arc::new(Store::open_in_memory().unwrap());
        let transcript = store.create_transcript(1, Sources::default()).unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        for (index, (raw, lang, _)) in paragraphs.iter().enumerate() {
            let start_ms = 1_000 * index as u64;
            let id = store.add_paragraph(transcript, Speaker::Me, start_ms, start_ms + 1_000, raw).unwrap();
            tx.send(Closed { id, speaker: Speaker::Me, text: (*raw).into(), lang: *lang }).unwrap();
        }
        drop(tx);
        let status = Arc::new(watch::channel(TranscriptionStatus::default()).0);
        store.add_word_entry("MTG", &["meeting".to_string()], Some(Lang::Ja), 1).unwrap();
        store.add_word_entry("PR", &["pull request".to_string()], None, 1).unwrap();
        let wordbook = Arc::new(SessionWordbook::load(store.clone(), false));
        let labelled = Arc::new(AtomicBool::new(false));
        polish_loop(store.clone(), Some(Ok(Arc::new(Echo))), wordbook, labelled, rx, status).await;
        let stored = store.get_transcript(transcript).unwrap().unwrap().paragraphs;
        let texts: Vec<&str> = stored.iter().map(|paragraph| paragraph.text.as_str()).collect();
        let expected: Vec<&str> = paragraphs.iter().map(|(_, _, text)| *text).collect();
        assert_eq!(texts, expected);
    }

    #[test]
    fn a_title_reply_keeps_its_first_line_without_quotes() {
        assert_eq!(clean_title("「📋 リリース計画の確認」\n説明です"), Some("📋 リリース計画の確認".into()));
        assert_eq!(clean_title("\n  \"週次定例\".\n"), Some("週次定例".into()));
        assert_eq!(clean_title("   \n"), None);
    }

    #[test]
    fn title_prompt_uses_the_interface_language_as_context_and_keeps_the_custom_instruction() {
        let prompt = title_system_prompt("Start with an emoji.", Lang::Ja);
        assert!(prompt.contains("Infer the main language spoken in the meeting"));
        assert!(prompt.contains("Japanese (ja) and is provided as context only"));
        assert!(prompt.ends_with("Start with an emoji."));
    }

    #[tokio::test]
    async fn a_typed_title_is_never_replaced_and_an_empty_one_is_named() {
        let store = Store::open_in_memory().unwrap();
        let status = Arc::new(watch::channel(TranscriptionStatus::default()).0);
        let named = store.create_transcript(1, Sources::default()).unwrap();
        store.add_paragraph(named, Speaker::Others, 0, 1_000, "来週のリリース計画を確認します").unwrap();
        let typed = store.create_transcript(2, Sources::default()).unwrap();
        store.add_paragraph(typed, Speaker::Others, 0, 1_000, "来週のリリース計画を確認します").unwrap();
        store.set_transcript_title(typed, "自分でつけた題").unwrap();
        let model = MockTextModel::default().script([Ok("📋 リリース計画の確認".to_string())]);
        name_untitled(&store, &model, "instruction", typed, &status).await;
        assert_eq!(model.calls(), 0, "a titled meeting never reaches the model");
        assert_eq!(store.get_transcript(typed).unwrap().unwrap().summary.title, "自分でつけた題");
        name_untitled(&store, &model, "instruction", named, &status).await;
        assert_eq!(store.get_transcript(named).unwrap().unwrap().summary.title, "📋 リリース計画の確認");
    }

    /// Plays a script per source as soon as it opens, then stays open until dropped. A test can also push more
    /// audio into an open source later, as its device would.
    struct ScriptedAudio {
        scripts: Mutex<Vec<(AudioSource, Vec<f32>)>>,
        /// Every source opened, in order.
        opened: Mutex<Vec<AudioSource>>,
        /// The streams still open, so a test can push audio into them.
        streams: Mutex<Vec<(AudioSource, ChunkSink)>>,
        /// This instance behind its `Arc`, so an open stream can leave `streams` when it is dropped.
        weak: OnceLock<Weak<ScriptedAudio>>,
        /// Sources whose open stream reports itself lost, as an unplugged device would.
        lost: Mutex<Vec<AudioSource>>,
    }

    impl ScriptedAudio {
        fn new(scripts: Vec<(AudioSource, Vec<f32>)>) -> Arc<Self> {
            let audio = Arc::new(Self {
                scripts: Mutex::new(scripts),
                opened: Mutex::new(Vec::new()),
                streams: Mutex::new(Vec::new()),
                weak: OnceLock::new(),
                lost: Mutex::new(Vec::new()),
            });
            audio.weak.set(Arc::downgrade(&audio)).ok();
            audio
        }

        /// Unplugs the device of `source`: its open stream reports itself lost. Plugged in again, it opens fine.
        fn unplug(&self, source: AudioSource, unplugged: bool) {
            let mut lost = self.lost.lock().unwrap();
            lost.retain(|s| *s != source);
            if unplugged {
                lost.push(source);
            }
        }

        /// Feeds audio into an open source's stream, as its device would.
        fn push(&self, source: AudioSource, samples: &[f32]) {
            let mut streams = self.streams.lock().unwrap();
            let i = streams.iter().rposition(|(s, _)| *s == source).expect("source open");
            for chunk in samples.chunks(1_600) {
                streams[i].1(chunk);
            }
        }
    }

    /// One open stream. Like a closed device, it stops carrying audio once the session drops it.
    struct Open {
        audio: Weak<ScriptedAudio>,
        source: AudioSource,
    }

    impl LiveInput for Open {
        fn lost(&self) -> Option<String> {
            let audio = self.audio.upgrade()?;
            audio.lost.lock().unwrap().contains(&self.source).then(|| "device unplugged".to_owned())
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            if let Some(audio) = self.audio.upgrade() {
                let mut streams = audio.streams.lock().unwrap();
                if let Some(i) = streams.iter().rposition(|(s, _)| *s == self.source) {
                    // Dropping the sink closes the stream's hold on the audio channel, as a device would.
                    drop(streams.swap_remove(i));
                }
            }
        }
    }

    impl LiveAudio for ScriptedAudio {
        fn open(&self, source: AudioSource, mut on_chunk: ChunkSink) -> Result<Box<dyn LiveInput>, RecorderError> {
            {
                let scripts = self.scripts.lock().unwrap();
                let (_, samples) = scripts.iter().find(|(s, _)| *s == source).ok_or(RecorderError::NoInputDevice)?;
                for chunk in samples.chunks(1_600) {
                    on_chunk(chunk);
                }
            }
            self.opened.lock().unwrap().push(source);
            self.streams.lock().unwrap().push((source, on_chunk));
            Ok(Box::new(Open { audio: self.weak.get().expect("behind an Arc").clone(), source }))
        }
    }

    fn speech(secs: f32) -> Vec<f32> {
        let mut v = vec![0.0; 8_000];
        v.extend((0..(16_000. * secs) as usize).map(|i| (i as f32 * 0.05).sin() * 0.3));
        v.extend(vec![0.0; 16_000]);
        v
    }

    /// A missing file reads as the defaults and nothing is written.
    fn config() -> Arc<ConfigStore> {
        Arc::new(ConfigStore::open(std::env::temp_dir().join("dito-transcription-test/never-written.toml")).unwrap())
    }

    async fn wait_until(
        handle: &TranscriptionHandle,
        done: impl Fn(&TranscriptionStatus) -> bool,
    ) -> TranscriptionStatus {
        let mut rx = handle.subscribe();
        let status = tokio::time::timeout(Duration::from_secs(5), rx.wait_for(|s| done(s))).await;
        status.expect("timed out").expect("actor alive").clone()
    }

    #[tokio::test]
    async fn a_session_with_the_microphone_keeps_each_speaker_in_its_own_polished_paragraph() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let config = config();
        let handle = spawn(TranscriptionDeps {
            audio: ScriptedAudio::new(vec![(AudioSource::System, speech(1.0)), (AudioSource::Microphone, speech(0.8))]),
            detectors: Arc::new(EnergyDetectors),
            engines: Arc::new(FixedEngines::new(
                Arc::new(MockTranscriber::new("えーと決済の件です")),
                Arc::new(MockTextModel::default()),
            )),
            config,
            store: store.clone(),
        });

        handle.start(Sources { system: true, microphone: true });
        let t = record_once(&handle, &store).await;
        assert!(t.summary.ended_ts.is_some());
        let mut speakers: Vec<_> = t.paragraphs.iter().map(|p| p.speaker).collect();
        speakers.sort_by_key(|s| *s == Speaker::Me);
        assert_eq!(speakers, vec![Speaker::Others, Speaker::Me]);
        for p in &t.paragraphs {
            assert_eq!(p.state, ParagraphState::Done);
            assert_eq!(p.raw, "えーと決済の件です");
            assert!(!p.text.contains("えーと"), "polished: {}", p.text);
        }
    }

    #[tokio::test]
    async fn a_resumed_session_appends_to_the_same_transcript_on_its_clock() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let started = crate::unix_seconds() - 600;
        let id = store.create_transcript(started, Sources::default()).unwrap();
        let first = store.add_paragraph(id, Speaker::Others, 0, 1_000, "最初の会議です").unwrap();
        store.finish_paragraph(first, "最初の会議です。", None).unwrap();
        store.end_transcript(id, 300_000, started + 300).unwrap();
        let handle = spawn(TranscriptionDeps {
            audio: ScriptedAudio::new(vec![(AudioSource::Microphone, speech(0.8))]),
            detectors: Arc::new(EnergyDetectors),
            engines: Arc::new(FixedEngines::new(
                Arc::new(MockTranscriber::new("続きの話です")),
                Arc::new(MockTextModel::default()),
            )),
            config: config(),
            store: store.clone(),
        });

        handle.resume(id, Sources { system: false, microphone: true });
        let recording = wait_until(&handle, |s| matches!(s.state, SessionState::Recording { .. })).await;
        assert_eq!(recording.state.transcript_id(), Some(id), "no new transcript is created");
        assert_eq!(recording.closed, 1, "the earlier paragraph counts as closed, so a view keeps showing it");
        let t = record_once(&handle, &store).await;
        assert_eq!(store.list_transcripts().unwrap().len(), 1);
        assert_eq!(t.paragraphs[0].id, first);
        let appended = &t.paragraphs[1..];
        assert!(!appended.is_empty());
        assert!(appended.iter().all(|p| p.start_ms >= 590_000), "timed from the meeting's start: {appended:?}");
        assert_eq!(t.summary.sources, Sources { system: true, microphone: true });
        assert!(t.summary.ended_ts.is_some());
        assert!(t.summary.length_ms.is_some_and(|ms| ms < 400_000), "the gap is not counted: {:?}", t.summary);
    }

    /// Starts nothing itself: waits for the session the caller started, stops it and returns what it stored.
    async fn record_once(handle: &TranscriptionHandle, store: &Store) -> crate::store::Transcript {
        let recording = wait_until(handle, |s| matches!(s.state, SessionState::Recording { .. })).await;
        let id = recording.state.transcript_id().unwrap();
        handle.stop();
        let done = wait_until(handle, |s| s.state == SessionState::Idle).await;
        assert_eq!(done.error, None);
        store.get_transcript(id).unwrap().unwrap()
    }

    /// Whatever the timing, a word once settled stays in exactly one place: the closed rows the status counts, then
    /// the live text. Checked on every status update a view could receive.
    #[tokio::test]
    async fn settled_words_never_move_back_or_show_twice_while_paragraphs_close() {
        use crate::provider::mock::{ToneTranscriber, tone};
        let lines = ["Shipping is Thursday.", "Tests are done but payments remain open for now.", "Next topic."];
        let mut audio = vec![0.0; 8_000];
        let mut script = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            audio.extend(tone(i, 1.5));
            audio.extend(vec![0.0; 12_000]);
            script.push((line.to_string(), 1.5));
        }
        let store = Arc::new(Store::open_in_memory().unwrap());
        let asr = ToneTranscriber::new(script, Duration::from_millis(5), Duration::ZERO);
        let handle = spawn(TranscriptionDeps {
            audio: ScriptedAudio::new(vec![(AudioSource::System, audio)]),
            detectors: Arc::new(EnergyDetectors),
            engines: Arc::new(FixedEngines::new(
                Arc::new(asr),
                Arc::new(MockTextModel::default().with_delay(Duration::from_millis(20))),
            )),
            config: config(),
            store: store.clone(),
        });
        let mut rx = handle.subscribe();
        handle.start(Sources::default());
        let mut settled = String::new();
        let mut stopping = false;
        let check = async {
            loop {
                rx.changed().await.expect("actor alive");
                let status = rx.borrow_and_update().clone();
                let Some(id) = status.state.transcript_id() else {
                    if stopping {
                        return;
                    }
                    continue;
                };
                let t = store.get_transcript(id).unwrap().unwrap();
                let mut now: String = t
                    .paragraphs
                    .iter()
                    .filter(|p| p.state != ParagraphState::Listening)
                    .take(status.closed)
                    .map(|p| p.raw.clone() + " ")
                    .collect();
                now.extend(status.live.iter().map(|l| l.settled.clone()));
                assert!(now.starts_with(&settled), "settled text went back:\n was {settled}\n now {now}");
                settled = now;
                if !stopping && status.closed >= 1 && status.live.is_empty() && settled.contains("Next") {
                    handle.stop();
                    stopping = true;
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(10), check).await.expect("timed out");
        assert!(settled.contains("Shipping is Thursday."), "{settled}");
    }

    #[tokio::test]
    async fn the_microphone_alone_is_recorded_as_the_user_without_opening_the_macs_audio() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let handle = spawn(TranscriptionDeps {
            // No system script: opening the Mac's audio would fail the start.
            audio: ScriptedAudio::new(vec![(AudioSource::Microphone, speech(0.8))]),
            detectors: Arc::new(EnergyDetectors),
            engines: Arc::new(FixedEngines::demo()),
            config: config(),
            store: store.clone(),
        });
        let mic_only = Sources { system: false, microphone: true };
        handle.start(mic_only);
        let t = record_once(&handle, &store).await;
        assert_eq!(t.summary.sources, mic_only);
        assert!(!t.paragraphs.is_empty());
        assert!(t.paragraphs.iter().all(|p| p.speaker == Speaker::Me));
    }

    #[tokio::test]
    async fn a_source_that_cannot_open_ends_the_start_with_an_error_and_no_transcript() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let handle = spawn(TranscriptionDeps {
            audio: ScriptedAudio::new(vec![(AudioSource::System, speech(0.5))]),
            detectors: Arc::new(EnergyDetectors),
            engines: Arc::new(FixedEngines::demo()),
            config: config(),
            store: store.clone(),
        });
        handle.start(Sources { system: true, microphone: true });
        let done = wait_until(&handle, |s| s.state == SessionState::Idle && s.error.is_some()).await;
        assert!(done.error.unwrap().contains("no input device"));
        assert!(store.list_transcripts().unwrap().is_empty());
    }

    #[tokio::test]
    async fn turning_a_source_off_and_on_again_keeps_recording_it() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let audio =
            ScriptedAudio::new(vec![(AudioSource::System, speech(1.0)), (AudioSource::Microphone, speech(0.8))]);
        let handle = spawn(TranscriptionDeps {
            audio: audio.clone(),
            detectors: Arc::new(EnergyDetectors),
            engines: Arc::new(FixedEngines::new(
                Arc::new(MockTranscriber::new("えーと決済の件です")),
                Arc::new(MockTextModel::default()),
            )),
            config: config(),
            store: store.clone(),
        });
        let both = Sources { system: true, microphone: true };
        let system_only = Sources { system: true, microphone: false };

        handle.start(both);
        let recording = wait_until(&handle, |s| matches!(s.state, SessionState::Recording { .. })).await;
        let id = recording.state.transcript_id().unwrap();

        handle.set_sources(system_only);
        wait_until(&handle, |s| matches!(&s.state, SessionState::Recording { sources, .. } if *sources == system_only))
            .await;
        // Neither source set: the session keeps the sources it records.
        handle.set_sources(Sources { system: false, microphone: false });
        tokio::time::sleep(Duration::from_millis(100)).await;
        let unchanged = handle.status();
        assert!(
            matches!(&unchanged.state, SessionState::Recording { sources, .. } if *sources == system_only),
            "{:?}",
            unchanged.state
        );

        handle.set_sources(both);
        wait_until(&handle, |s| matches!(&s.state, SessionState::Recording { sources, .. } if *sources == both)).await;
        handle.stop();
        let done = wait_until(&handle, |s| s.state == SessionState::Idle).await;
        assert_eq!(done.error, None);
        let t = store.get_transcript(id).unwrap().unwrap();
        assert!(t.summary.ended_ts.is_some());
        assert_eq!(t.summary.sources, both);
        let opened = audio.opened.lock().unwrap();
        assert_eq!(opened.iter().filter(|s| **s == AudioSource::Microphone).count(), 2, "{opened:?}");
    }

    fn demo_deps(audio: Arc<ScriptedAudio>, store: Arc<Store>) -> TranscriptionDeps {
        TranscriptionDeps {
            audio,
            detectors: Arc::new(EnergyDetectors),
            engines: Arc::new(FixedEngines::new(
                Arc::new(MockTranscriber::new("えーと決済の件です")),
                Arc::new(MockTextModel::default()),
            )),
            config: config(),
            store,
        }
    }

    #[tokio::test]
    async fn a_lost_source_turns_off_and_the_other_keeps_recording() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let audio =
            ScriptedAudio::new(vec![(AudioSource::System, speech(1.0)), (AudioSource::Microphone, speech(0.8))]);
        let handle = spawn(demo_deps(audio.clone(), store));
        let both = Sources { system: true, microphone: true };
        let system_only = Sources { system: true, microphone: false };
        handle.start(both);
        wait_until(&handle, |s| matches!(s.state, SessionState::Recording { .. })).await;

        audio.unplug(AudioSource::Microphone, true);
        let lost = wait_until(&handle, |s| {
            matches!(&s.state, SessionState::Recording { sources, segment_start: Some(_), .. } if *sources == system_only)
        })
        .await;
        assert_eq!(lost.error.as_deref(), Some(config().get().general.lang().tr(Tr::TranscriptionMicrophoneLost)));
        handle.stop();
        wait_until(&handle, |s| s.state == SessionState::Idle).await;
    }

    #[tokio::test]
    async fn losing_the_last_source_pauses_and_the_resume_opens_it_again() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let audio = ScriptedAudio::new(vec![(AudioSource::Microphone, speech(0.8))]);
        let handle = spawn(demo_deps(audio.clone(), store));
        let mic_only = Sources { system: false, microphone: true };
        handle.start(mic_only);
        wait_until(&handle, |s| matches!(s.state, SessionState::Recording { .. })).await;

        audio.unplug(AudioSource::Microphone, true);
        let paused = wait_until(&handle, |s| {
            matches!(&s.state, SessionState::Recording { sources, segment_start: None, .. } if *sources == mic_only)
        })
        .await;
        assert_eq!(paused.error.as_deref(), Some(config().get().general.lang().tr(Tr::TranscriptionMicrophoneLost)));

        audio.unplug(AudioSource::Microphone, false);
        handle.toggle_pause();
        wait_until(&handle, |s| matches!(&s.state, SessionState::Recording { segment_start: Some(_), .. })).await;
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(
            matches!(handle.status().state, SessionState::Recording { segment_start: Some(_), .. }),
            "the reopened stream keeps recording"
        );
        let opened = audio.opened.lock().unwrap().clone();
        assert_eq!(opened, vec![AudioSource::Microphone, AudioSource::Microphone]);
        handle.stop();
        wait_until(&handle, |s| s.state == SessionState::Idle).await;
    }

    #[tokio::test]
    async fn a_source_turned_on_while_paused_after_a_loss_opens_once_on_resume() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let audio =
            ScriptedAudio::new(vec![(AudioSource::System, speech(1.0)), (AudioSource::Microphone, speech(0.8))]);
        let handle = spawn(demo_deps(audio.clone(), store));
        let mic_only = Sources { system: false, microphone: true };
        let both = Sources { system: true, microphone: true };
        handle.start(mic_only);
        wait_until(&handle, |s| matches!(s.state, SessionState::Recording { .. })).await;

        audio.unplug(AudioSource::Microphone, true);
        wait_until(&handle, |s| matches!(&s.state, SessionState::Recording { segment_start: None, .. })).await;
        handle.set_sources(both);
        let shown =
            wait_until(&handle, |s| matches!(&s.state, SessionState::Recording { sources, .. } if *sources == both))
                .await;
        assert!(matches!(shown.state, SessionState::Recording { segment_start: None, .. }), "still paused");
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert_eq!(
            *audio.opened.lock().unwrap(),
            vec![AudioSource::Microphone, AudioSource::System],
            "the new source opens silent; the lost one waits for the resume"
        );

        audio.unplug(AudioSource::Microphone, false);
        handle.toggle_pause();
        wait_until(&handle, |s| matches!(&s.state, SessionState::Recording { segment_start: Some(_), .. })).await;
        tokio::time::sleep(Duration::from_millis(600)).await;
        let mut opened = audio.opened.lock().unwrap().clone();
        opened.sort_by_key(|s| *s == AudioSource::System);
        assert_eq!(opened, vec![AudioSource::Microphone, AudioSource::Microphone, AudioSource::System]);
        assert_eq!(audio.streams.lock().unwrap().len(), 2, "one stream per source");
        handle.stop();
        wait_until(&handle, |s| s.state == SessionState::Idle).await;
    }

    /// The elapsed time a view computes from the status alone: ticking while recording, frozen while paused,
    /// and continuing from where it froze after the resume.
    #[tokio::test]
    async fn pausing_freezes_the_recorded_time_until_the_session_resumes() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let handle = spawn(TranscriptionDeps {
            audio: ScriptedAudio::new(vec![(AudioSource::Microphone, speech(0.1))]),
            detectors: Arc::new(EnergyDetectors),
            engines: Arc::new(FixedEngines::demo()),
            config: config(),
            store: store.clone(),
        });
        handle.start(Sources { system: false, microphone: true });
        let recording =
            wait_until(&handle, |s| matches!(&s.state, SessionState::Recording { segment_start: Some(_), .. })).await;
        let id = recording.state.transcript_id().unwrap();

        // A second passes while recording, so the open stretch holds at least that much time.
        tokio::time::sleep(Duration::from_millis(1_100)).await;
        handle.toggle_pause();
        let paused =
            wait_until(&handle, |s| matches!(&s.state, SessionState::Recording { segment_start: None, .. })).await;
        let frozen = paused.state.elapsed_ms();
        assert!(frozen >= 1_000, "{frozen}");

        // However long the pause lasts, the recorded time stays where it froze.
        tokio::time::sleep(Duration::from_millis(1_100)).await;
        assert_eq!(handle.status().state.elapsed_ms(), frozen);

        handle.toggle_pause();
        wait_until(&handle, |s| matches!(&s.state, SessionState::Recording { segment_start: Some(_), .. })).await;
        tokio::time::sleep(Duration::from_millis(1_100)).await;
        let resumed = handle.status().state.elapsed_ms();
        assert!(resumed >= frozen + 1_000, "{resumed} < {frozen} + 1000");

        handle.stop();
        let done = wait_until(&handle, |s| s.state == SessionState::Idle).await;
        assert_eq!(done.error, None);
        assert!(store.get_transcript(id).unwrap().unwrap().summary.ended_ts.is_some());
    }

    /// Audio that arrives while paused never becomes words; the same audio does after the resume.
    #[tokio::test]
    async fn audio_while_paused_is_dropped_and_recording_resumes() {
        use crate::provider::mock::{ToneTranscriber, tone};

        let mut script = vec![0.0; 8_000];
        script.extend(tone(0, 1.5));
        script.extend(vec![0.0; 12_000]);
        let mut pushed = tone(1, 1.5);
        pushed.extend(vec![0.0; 12_000]);
        let lines = vec![("First words.".to_string(), 1.5), ("Second words.".to_string(), 1.5)];
        let store = Arc::new(Store::open_in_memory().unwrap());
        let asr = ToneTranscriber::new(lines, Duration::from_millis(5), Duration::ZERO);
        let audio = ScriptedAudio::new(vec![(AudioSource::System, script)]);
        let handle = spawn(TranscriptionDeps {
            audio: audio.clone(),
            detectors: Arc::new(EnergyDetectors),
            engines: Arc::new(FixedEngines::new(Arc::new(asr), Arc::new(MockTextModel::default()))),
            config: config(),
            store: store.clone(),
        });
        handle.start(Sources { system: true, microphone: false });
        let recording = wait_until(&handle, |s| matches!(s.state, SessionState::Recording { .. })).await;
        let id = recording.state.transcript_id().unwrap();
        // Whether a word is settled: on screen as live text, or stored with its paragraph.
        let heard = |s: &TranscriptionStatus, word: &str| {
            s.live.iter().any(|l| l.settled.contains(word))
                || store.get_transcript(id).unwrap().unwrap().paragraphs.iter().any(|p| p.raw.contains(word))
        };
        // Nothing is in flight once the first words have settled.
        wait_until(&handle, |s| heard(s, "First")).await;

        handle.toggle_pause();
        wait_until(&handle, |s| matches!(&s.state, SessionState::Recording { segment_start: None, .. })).await;
        audio.push(AudioSource::System, &pushed);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!heard(&handle.status(), "Second"), "audio while paused became words");

        handle.toggle_pause();
        wait_until(&handle, |s| matches!(&s.state, SessionState::Recording { segment_start: Some(_), .. })).await;
        audio.push(AudioSource::System, &pushed);
        wait_until(&handle, |s| heard(s, "Second")).await;

        handle.stop();
        let done = wait_until(&handle, |s| s.state == SessionState::Idle).await;
        assert_eq!(done.error, None);
        let t = store.get_transcript(id).unwrap().unwrap();
        assert!(t.summary.ended_ts.is_some());
        assert!(t.paragraphs.iter().any(|p| p.raw.contains("Second")), "resume recorded nothing: {:?}", t.paragraphs);
    }

    #[tokio::test]
    async fn stopping_while_paused_finishes_the_session() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let handle = spawn(TranscriptionDeps {
            audio: ScriptedAudio::new(vec![(AudioSource::System, speech(1.0)), (AudioSource::Microphone, speech(0.8))]),
            detectors: Arc::new(EnergyDetectors),
            engines: Arc::new(FixedEngines::demo()),
            config: config(),
            store: store.clone(),
        });
        handle.start(Sources { system: true, microphone: false });
        let recording = wait_until(&handle, |s| matches!(s.state, SessionState::Recording { .. })).await;
        let id = recording.state.transcript_id().unwrap();

        handle.toggle_pause();
        wait_until(&handle, |s| matches!(&s.state, SessionState::Recording { segment_start: None, .. })).await;
        // The sources still follow the user while paused; the new one just stays silent until the resume.
        let mic_only = Sources { system: false, microphone: true };
        handle.set_sources(mic_only);
        wait_until(&handle, |s| {
            matches!(&s.state, SessionState::Recording { sources, segment_start: None, .. } if *sources == mic_only)
        })
        .await;

        handle.stop();
        let done = wait_until(&handle, |s| s.state == SessionState::Idle).await;
        assert_eq!(done.error, None);
        let t = store.get_transcript(id).unwrap().unwrap();
        assert!(t.summary.ended_ts.is_some());
        assert_eq!(t.summary.sources, Sources { system: true, microphone: true });
    }
}
