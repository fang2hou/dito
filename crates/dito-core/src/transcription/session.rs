//! The actor that owns the running transcription session.
//!
//! Data flows one way. Source threads cut each stream into utterances ([`super::source`]); one recognizer task turns
//! their audio into text ([`super::recognizer`]); this loop owns everything shown while recording (the open
//! paragraph and the words still being heard) and every paragraph it closes, and publishes both together: a closed
//! paragraph is stored before the status update that drops it from [`TranscriptionStatus::live`], and that update
//! also bumps the revision and counts it in [`TranscriptionStatus::closed`]. The store can run ahead of the status
//! a view last received, so a view shows only the first `closed` closed paragraphs of the session: each word is then
//! in exactly one place.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use super::live::{Hearing, LiveText};
use super::paragraphs::{self, Paragraph, Paragrapher};
use super::prompt::{CONTEXT_PARAGRAPHS, ContextParagraph, system_prompt, worth_polishing};
use super::recognizer::{Job, Recognized, Recognizer, Utterance};
use super::source::{SourceEvent, detect};
use super::{AudioSource, LiveAudio, Sources, Speaker, VoiceDetectors};
use crate::config::ConfigStore;
use crate::factory::Engines;
use crate::pipeline::polish_wordbook;
use crate::provider::{PolishRequest, Polisher};
use crate::store::Store;

const IDLE_TICK: Duration = Duration::from_millis(250);

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
    Recording {
        id: i64,
        started_ts: i64,
        sources: Sources,
    },
    /// The audio is closed; the last utterances are being recognized and polished.
    Finishing {
        id: i64,
    },
}

impl SessionState {
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
    /// How many paragraphs of the running session have closed, as of `live`. Paragraphs close in the order they
    /// started.
    pub closed: usize,
}

enum Cmd {
    Start { sources: Sources },
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
        let _ = self.tx.send(Cmd::Start { sources });
    }

    /// The session finishes the utterances it already heard before it ends.
    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
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
    if let Err(e) = deps.store.recover_transcripts(now_ts()) {
        tracing::warn!("could not close unfinished transcripts: {e}");
    }
    let (tx, rx) = mpsc::unbounded_channel();
    let (status_tx, status_rx) = watch::channel(TranscriptionStatus::default());
    tokio::spawn(actor(Arc::new(deps), rx, Arc::new(status_tx)));
    TranscriptionHandle { tx, status: status_rx }
}

type Status = Arc<watch::Sender<TranscriptionStatus>>;

async fn actor(deps: Arc<TranscriptionDeps>, mut rx: mpsc::UnboundedReceiver<Cmd>, status: Status) {
    let mut running: Option<CancellationToken> = None;
    while let Some(cmd) = rx.recv().await {
        match cmd {
            Cmd::Start { sources } => {
                if status.borrow().state != SessionState::Idle {
                    continue;
                }
                let stop = CancellationToken::new();
                running = Some(stop.clone());
                status.send_modify(|s| {
                    s.state = SessionState::Starting;
                    s.error = None;
                });
                tokio::spawn(run(deps.clone(), sources, stop, status.clone()));
            }
            Cmd::Stop => {
                if let Some(stop) = running.take() {
                    stop.cancel();
                }
            }
        }
    }
}

async fn run(deps: Arc<TranscriptionDeps>, sources: Sources, stop: CancellationToken, status: Status) {
    let result = record(&deps, sources, &stop, &status).await;
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

fn now_ts() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

struct Closed {
    id: i64,
    speaker: Speaker,
    text: String,
}

struct Source {
    speaker: Speaker,
    speaking: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<()>,
}

async fn record(
    deps: &TranscriptionDeps,
    sources: Sources,
    stop: &CancellationToken,
    status: &Status,
) -> Result<(), String> {
    let cfg = deps.config.get();
    let asr = deps.engines.transcriber(&cfg).await.map_err(|e| e.to_string())?;
    let polisher = match deps.engines.polisher(&cfg).await {
        Ok(p) => p.map(Ok),
        Err(e) => Some(Err(e.to_string())),
    };
    let words = deps.store.words(None).unwrap_or_default();
    let terms = polish_wordbook(&cfg, &words).to_vec();
    let mut wanted = Vec::new();
    if sources.system {
        wanted.push((AudioSource::System, Speaker::Others));
    }
    if sources.microphone {
        wanted.push((AudioSource::Microphone, Speaker::Me));
    }
    if wanted.is_empty() {
        return Err("no audio source selected".into());
    }
    let detectors = wanted.iter().map(|_| deps.detectors.create()).collect::<Result<Vec<_>, _>>()?;
    if stop.is_cancelled() {
        return Ok(());
    }

    let session_start = Instant::now();
    let (heard_tx, mut heard_rx) = mpsc::unbounded_channel::<SourceEvent>();
    let mut inputs = Vec::new();
    let mut taps = Vec::new();
    for ((source, speaker), detector) in wanted.into_iter().zip(detectors) {
        // Stamped on the audio thread: the detector thread may fall behind, and its clock would then see gaps
        // that are only a backlog.
        let (chunk_tx, chunk_rx) = std_mpsc::channel::<(Instant, Vec<f32>)>();
        let input = deps
            .audio
            .open(
                source,
                Box::new(move |chunk| {
                    let _ = chunk_tx.send((Instant::now(), chunk.to_vec()));
                }),
            )
            .map_err(|e| e.to_string())?;
        let speaking = Arc::new(AtomicBool::new(false));
        let (flag, tx) = (speaking.clone(), heard_tx.clone());
        let thread = std::thread::Builder::new()
            .name("dito-transcription-vad".into())
            .spawn(move || detect(detector, chunk_rx, speaker, session_start, &flag, &tx))
            .map_err(|e| e.to_string())?;
        inputs.push(input);
        taps.push(Source { speaker, speaking, thread });
    }
    drop(heard_tx);

    let started_ts = now_ts();
    let id = deps.store.create_transcript(started_ts, sources).map_err(|e| e.to_string())?;
    status.send_modify(|s| {
        s.state = SessionState::Recording { id, started_ts, sources };
        s.revision += 1;
        s.live.clear();
        s.closed = 0;
    });

    let (job_tx, job_rx) = mpsc::unbounded_channel::<Job>();
    let (text_tx, mut text_rx) = mpsc::unbounded_channel::<Recognized>();
    let recognizer = Recognizer { asr, language: cfg.general.preferred_language.code().map(str::to_string), words };
    let recognizer = tokio::spawn(recognizer.run(job_rx, text_tx));
    let (polish_tx, polish_rx) = mpsc::unbounded_channel::<Closed>();
    let polish_task =
        tokio::spawn(polish_loop(deps.store.clone(), polisher, terms, sources.both(), polish_rx, status.clone()));
    let mut session = Session {
        store: &deps.store,
        id,
        status,
        polish_tx,
        paragraphs: Paragrapher::default(),
        open_id: None,
        hearing: Hearing::default(),
        published: Vec::new(),
        closed: 0,
    };
    let mut inputs = Some(inputs);
    // `None` once every source has ended: the recognizer then finishes what it has and closes `text_rx`.
    let mut jobs = Some(job_tx);
    let mut sources_open = true;
    let mut tick = tokio::time::interval(IDLE_TICK);
    loop {
        tokio::select! {
            _ = stop.cancelled(), if inputs.is_some() => {
                // Dropping the inputs ends the streams; the source threads then flush what they hold.
                inputs = None;
                status.send_modify(|s| s.state = SessionState::Finishing { id });
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
    for source in taps {
        let _ = tokio::task::spawn_blocking(move || source.thread.join()).await;
    }
    deps.store.end_transcript(id, now_ts()).map_err(|e| e.to_string())?;
    Ok(())
}

struct Session<'a> {
    store: &'a Store,
    id: i64,
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
            Recognized::Final { speaker, seq, start_ms, end_ms, text } => {
                let start_ms = self.hearing.start_of(speaker, seq).map_or(start_ms, |s| s.min(start_ms));
                self.hearing.finished_utterance(speaker, seq);
                let Some(text) = text else { return false };
                for p in self.paragraphs.push(paragraphs::Utterance { speaker, start_ms, end_ms, text }) {
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
        let live = self.hearing.view(self.paragraphs.open());
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
        let stored = match self.open_id {
            Some(id) => self.store.extend_paragraph(id, p.start_ms, p.end_ms, &p.text).map(|()| id),
            None => self.store.add_paragraph(self.id, p.speaker, p.start_ms, p.end_ms, &p.text),
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
            let _ = self.polish_tx.send(Closed { id, speaker: p.speaker, text: p.text });
            self.closed += 1;
        }
    }
}

async fn polish_loop(
    store: Arc<Store>,
    polisher: Option<Result<Arc<dyn Polisher>, String>>,
    terms: Vec<String>,
    labelled: bool,
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
                let system = system_prompt(&p.text, context.make_contiguous(), labelled, &terms);
                match polisher.polish(&PolishRequest { system: &system, examples: &[], text: &p.text }).await {
                    Ok(out) if !out.trim().is_empty() => (out.trim().to_string(), None),
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
    use std::sync::Mutex;

    use super::*;
    use crate::controller::RecorderError;
    use crate::factory::FixedEngines;
    use crate::provider::mock::{MockPolisher, MockTranscriber};
    use crate::store::{ParagraphState, Speaker};
    use crate::transcription::{ChunkSink, EnergyDetectors, LiveInput};

    /// Plays a script per source as soon as it opens, then stays open until dropped.
    struct ScriptedAudio {
        scripts: Mutex<Vec<(AudioSource, Vec<f32>)>>,
    }

    struct Open(#[allow(dead_code)] ChunkSink);
    impl LiveInput for Open {}

    impl LiveAudio for ScriptedAudio {
        fn open(&self, source: AudioSource, mut on_chunk: ChunkSink) -> Result<Box<dyn LiveInput>, RecorderError> {
            let scripts = self.scripts.lock().unwrap();
            let (_, samples) = scripts.iter().find(|(s, _)| *s == source).ok_or(RecorderError::NoInputDevice)?;
            for chunk in samples.chunks(1_600) {
                on_chunk(chunk);
            }
            Ok(Box::new(Open(on_chunk)))
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
            audio: Arc::new(ScriptedAudio {
                scripts: Mutex::new(vec![(AudioSource::System, speech(1.0)), (AudioSource::Microphone, speech(0.8))]),
            }),
            detectors: Arc::new(EnergyDetectors),
            engines: Arc::new(FixedEngines::new(
                Arc::new(MockTranscriber::new("えーと決済の件です")),
                Arc::new(MockPolisher::default()),
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
            audio: Arc::new(ScriptedAudio { scripts: Mutex::new(vec![(AudioSource::System, audio)]) }),
            detectors: Arc::new(EnergyDetectors),
            engines: Arc::new(FixedEngines::new(
                Arc::new(asr),
                Arc::new(MockPolisher::default().with_delay(Duration::from_millis(20))),
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
            audio: Arc::new(ScriptedAudio { scripts: Mutex::new(vec![(AudioSource::Microphone, speech(0.8))]) }),
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
            audio: Arc::new(ScriptedAudio { scripts: Mutex::new(vec![(AudioSource::System, speech(0.5))]) }),
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
}
