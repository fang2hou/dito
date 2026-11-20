use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::config::{Config, ConfigStore, DEFAULT_ASR_LOCAL, DEFAULT_TEXT_LOCAL, InsertMethod};
use crate::controller::{
    CapturedSelection, Env, FocusWarmer, Frontmost, MusicMute, Recorder, RecorderError, ResultWrite, SelectionSource,
    Sink,
};
use crate::error::ProviderResult;
use crate::factory::{Engines, FixedEngines};
use crate::intent::{Decision, Intent};
use crate::provider::mock::{MockRouter, MockTextModel, MockTranscriber, decision};
use crate::provider::{Router, TextModel, TextRequest, Transcriber};
use crate::store::{CommandOutcome, HistoryFilter, InstructionOrigin, SourceKind, ToolChoice};
use crate::surrounding::Surrounding;

struct FakeRecorder {
    fail: bool,
    clip: AudioClip,
    starts: Arc<AtomicUsize>,
}
impl Recorder for FakeRecorder {
    fn start(&self) -> Result<(), RecorderError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        if self.fail { Err(RecorderError::Device("no permission".into())) } else { Ok(()) }
    }
    fn stop(&self) -> AudioClip {
        self.clip.clone()
    }
    fn level(&self) -> f32 {
        0.5
    }
}

#[derive(Default)]
struct FakeSink {
    delivered: Mutex<Vec<String>>,
    copied: Mutex<Vec<String>>,
    no_target: bool,
    gate: Option<Gate>,
}
impl Sink for FakeSink {
    fn copy(&self, text: &str) -> Delivery {
        self.copied.lock().unwrap().push(text.into());
        Delivery::Copied
    }

    fn deliver(&self, text: &str, _: InsertMethod) -> Delivery {
        self.delivered.lock().unwrap().push(text.into());
        if let Some(gate) = &self.gate {
            let _ = gate.entered.send(());
            // Dropping the test's end releases a delivery that nobody waits for any more.
            let _ = gate.release.lock().unwrap().recv();
        }
        if self.no_target { Delivery::Copied } else { Delivery::Inserted }
    }
}

/// As slow as the real sink, which sleeps while it polls focus: reports that it started, then blocks until released.
struct Gate {
    entered: tokio::sync::mpsc::UnboundedSender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
}

struct GateControl {
    entered: tokio::sync::mpsc::UnboundedReceiver<()>,
    release: std::sync::mpsc::Sender<()>,
}

struct SelectionGate {
    entered: tokio::sync::mpsc::UnboundedSender<usize>,
    finished: tokio::sync::mpsc::UnboundedSender<usize>,
    releases: Mutex<HashMap<usize, std::sync::mpsc::Receiver<()>>>,
}

struct SelectionGateControl {
    entered: tokio::sync::mpsc::UnboundedReceiver<usize>,
    finished: tokio::sync::mpsc::UnboundedReceiver<usize>,
    releases: HashMap<usize, std::sync::mpsc::Sender<()>>,
}

impl SelectionGateControl {
    async fn entered(&mut self, capture: usize) {
        assert_eq!(self.entered.recv().await, Some(capture));
    }

    async fn finished(&mut self, capture: usize) {
        assert_eq!(self.finished.recv().await, Some(capture));
    }

    fn release(&mut self, capture: usize) {
        self.releases.remove(&capture).expect("capture is gated").send(()).expect("capture is still waiting");
    }
}

fn selection_gate(captures: &[usize]) -> (SelectionGate, SelectionGateControl) {
    let (entered_tx, entered_rx) = tokio::sync::mpsc::unbounded_channel();
    let (finished_tx, finished_rx) = tokio::sync::mpsc::unbounded_channel();
    let (mut receivers, mut senders) = (HashMap::new(), HashMap::new());
    for capture in captures {
        let (release, wait) = std::sync::mpsc::channel();
        receivers.insert(*capture, wait);
        senders.insert(*capture, release);
    }
    (
        SelectionGate { entered: entered_tx, finished: finished_tx, releases: Mutex::new(receivers) },
        SelectionGateControl { entered: entered_rx, finished: finished_rx, releases: senders },
    )
}

/// What a command run replaces, shared with the source that captured it.
type Replaced = Arc<Mutex<Vec<String>>>;

struct FakeSelection {
    text: String,
    surrounding: Option<Surrounding>,
    editable: bool,
    replaced: Replaced,
}
impl CapturedSelection for FakeSelection {
    fn text(&self) -> &str {
        &self.text
    }
    fn surrounding(&self) -> Option<&Surrounding> {
        self.surrounding.as_ref()
    }
    fn editable(&self) -> bool {
        self.editable
    }
    fn replace(&self, text: &str, _: InsertMethod) -> Delivery {
        self.replaced.lock().unwrap().push(text.into());
        Delivery::Inserted
    }
}

/// Captures when there is a selection or text around the caret, as the real source does.
struct FakeSelectionSource {
    text: Option<&'static str>,
    surrounding: Option<Surrounding>,
    /// What every reading after the first returns: `Some(None)` fails, `Some(Some(text))` selects `text` with the
    /// surrounding of [`later_around`]. `None` repeats the first reading.
    later: Option<Option<&'static str>>,
    /// Whether the first reading, and the later ones, can be written over.
    editable: bool,
    later_editable: bool,
    replaced: Replaced,
    captures: Arc<AtomicUsize>,
    readings: Option<Vec<Option<&'static str>>>,
    gate: Option<SelectionGate>,
}
impl SelectionSource for FakeSelectionSource {
    fn capture(&self) -> Option<Arc<dyn CapturedSelection>> {
        let capture = self.captures.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(gate) = &self.gate {
            let release = gate.releases.lock().unwrap().remove(&capture);
            let Some(release) = release else {
                return self.reading(capture);
            };
            let _ = gate.entered.send(capture);
            release.recv().expect("selection release was dropped");
            let _ = gate.finished.send(capture);
        }
        self.reading(capture)
    }
}

impl FakeSelectionSource {
    fn reading(&self, capture: usize) -> Option<Arc<dyn CapturedSelection>> {
        if let Some(reading) = self.readings.as_ref().and_then(|readings| readings.get(capture - 1)) {
            return reading.map(|text| {
                Arc::new(FakeSelection {
                    text: text.into(),
                    surrounding: (capture > 1).then(later_around).flatten(),
                    editable: self.later_editable,
                    replaced: self.replaced.clone(),
                }) as Arc<dyn CapturedSelection>
            });
        }
        if capture > 1
            && let Some(later) = self.later
        {
            return later.map(|text| {
                Arc::new(FakeSelection {
                    text: text.into(),
                    surrounding: later_around(),
                    editable: self.later_editable,
                    replaced: self.replaced.clone(),
                }) as Arc<dyn CapturedSelection>
            });
        }
        if self.text.is_none() && self.surrounding.is_none() {
            return None;
        }
        Some(Arc::new(FakeSelection {
            text: self.text.unwrap_or_default().into(),
            surrounding: self.surrounding.clone(),
            editable: self.editable,
            replaced: self.replaced.clone(),
        }))
    }
}

fn later_around() -> Option<Surrounding> {
    Some(Surrounding::Around { before: "Hello, ".into(), after: " Bye.".into() })
}

/// Records every request a tool sends to the model, as (system prompt, user message).
#[derive(Default)]
struct SpyTextModel {
    seen: Mutex<Vec<(String, String)>>,
}
#[async_trait::async_trait]
impl TextModel for Arc<SpyTextModel> {
    async fn polish(&self, req: &TextRequest<'_>) -> ProviderResult<String> {
        self.seen.lock().unwrap().push((req.system.into(), req.text.into()));
        Ok("result".into())
    }
}

/// The engine warm-ups that were started, and those still running.
#[derive(Default)]
struct WarmUps {
    started: AtomicUsize,
    running: AtomicUsize,
}

struct RunningWarmUp(Arc<WarmUps>);
impl Drop for RunningWarmUp {
    fn drop(&mut self) {
        self.0.running.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The rig's engines with a warm-up that never finishes, so no test passes if a run waits for one.
struct EndlessWarmUp {
    engines: FixedEngines,
    warm_ups: Arc<WarmUps>,
}

#[async_trait::async_trait]
impl Engines for EndlessWarmUp {
    async fn transcriber(&self, cfg: &Config) -> ProviderResult<Arc<dyn Transcriber>> {
        self.engines.transcriber(cfg).await
    }

    async fn text_model(&self, cfg: &Config) -> ProviderResult<Option<Arc<dyn TextModel>>> {
        self.engines.text_model(cfg).await
    }

    async fn router(&self, cfg: &Config) -> ProviderResult<Arc<dyn Router>> {
        self.engines.router(cfg).await
    }

    async fn warm_up(&self, _: &Config) {
        self.warm_ups.started.fetch_add(1, Ordering::SeqCst);
        self.warm_ups.running.fetch_add(1, Ordering::SeqCst);
        let _running = RunningWarmUp(self.warm_ups.clone());
        std::future::pending::<()>().await;
    }
}

struct FakeFront;
impl Frontmost for FakeFront {
    fn frontmost(&self) -> AppInfo {
        AppInfo { name: "Slack".into(), bundle_id: "com.tinyspeck.slackmacgap".into() }
    }
}

struct NoopFocusWarmer;
impl FocusWarmer for NoopFocusWarmer {
    fn warm(&self) {}
}

#[derive(Default, Clone)]
struct FakeMusicMute {
    events: Arc<Mutex<Vec<&'static str>>>,
}

impl FakeMusicMute {
    fn log(&self) -> Vec<String> {
        self.events.lock().unwrap().iter().map(|s| s.to_string()).collect()
    }
}

struct MuteGuardLog(Arc<Mutex<Vec<&'static str>>>);
impl Drop for MuteGuardLog {
    fn drop(&mut self) {
        self.0.lock().unwrap().push("unmute");
    }
}
impl MuteSession for MuteGuardLog {}

impl MusicMute for FakeMusicMute {
    fn mute_if_active(&self) -> Box<dyn MuteSession> {
        self.events.lock().unwrap().push("mute");
        Box::new(MuteGuardLog(self.events.clone()))
    }
}

struct RigOptions {
    mic_fails: bool,
    no_insertion_target: bool,
    audio_dir_blocked: bool,
    mode: RecordMode,
    music_mute: MusicMuteMode,
    gated_delivery: bool,
    selection: Option<&'static str>,
    surrounding: Option<Surrounding>,
    /// The selection the readings after the first return, see `FakeSelectionSource::later`.
    later_selection: Option<Option<&'static str>>,
    /// Whether the first reading, and the later ones, can be written over.
    editable: bool,
    later_editable: bool,
    selection_reads: Option<Vec<Option<&'static str>>>,
    selection_gate: Option<SelectionGate>,
    router: MockRouter,
}

impl Default for RigOptions {
    fn default() -> Self {
        Self {
            mic_fails: false,
            no_insertion_target: false,
            audio_dir_blocked: false,
            mode: RecordMode::Toggle,
            music_mute: MusicMuteMode::Off,
            gated_delivery: false,
            selection: None,
            surrounding: None,
            later_selection: None,
            editable: true,
            later_editable: true,
            selection_reads: None,
            selection_gate: None,
            router: MockRouter::default(),
        }
    }
}

struct Rig {
    h: DictationHandle,
    sink: Arc<FakeSink>,
    replaced: Replaced,
    store: Arc<Store>,
    config: Arc<ConfigStore>,
    asr: Arc<MockTranscriber>,
    router: Arc<MockRouter>,
    warm_ups: Arc<WarmUps>,
    mute: FakeMusicMute,
    /// How often the microphone was opened and the selection was read.
    recordings: Arc<AtomicUsize>,
    captures: Arc<AtomicUsize>,
    gate: Option<GateControl>,
    _dir: tempfile::TempDir,
}

impl Rig {
    async fn delivery_started(&mut self) {
        let gate = self.gate.as_mut().expect("the rig was built with gated_delivery");
        tokio::time::timeout(Duration::from_secs(5), gate.entered.recv())
            .await
            .expect("timed out waiting for delivery to start")
            .expect("the sink was dropped");
    }

    fn release_delivery(&self) {
        let gate = self.gate.as_ref().expect("the rig was built with gated_delivery");
        gate.release.send(()).expect("the sink was dropped");
    }
}

fn rig(asr: MockTranscriber, polish: impl TextModel + 'static, options: RigOptions) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let config = Arc::new(ConfigStore::open(dir.path().join("c.toml")).unwrap());
    config
        .update(|c| {
            c.general.record_mode = options.mode;
            c.general.music_mute = options.music_mute;
        })
        .unwrap();
    let store = Arc::new(Store::open_in_memory().unwrap());
    let (gate, gate_control) = if options.gated_delivery {
        let (entered_tx, entered_rx) = tokio::sync::mpsc::unbounded_channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        (
            Some(Gate { entered: entered_tx, release: Mutex::new(release_rx) }),
            Some(GateControl { entered: entered_rx, release: release_tx }),
        )
    } else {
        (None, None)
    };
    let sink = Arc::new(FakeSink { no_target: options.no_insertion_target, gate, ..Default::default() });
    let asr = Arc::new(asr);
    let music_mute = FakeMusicMute::default();
    let audio_dir = dir.path().join("audio");
    if options.audio_dir_blocked {
        std::fs::write(&audio_dir, "not a directory").unwrap();
    }
    let replaced = Replaced::default();
    let (recordings, captures) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let mut engines = FixedEngines::new(asr.clone(), Arc::new(polish));
    let router = Arc::new(options.router);
    engines.router = router.clone();
    let warm_ups = Arc::new(WarmUps::default());
    let h = spawn(Deps {
        env: Env {
            recorder: Arc::new(FakeRecorder {
                fail: options.mic_fails,
                clip: AudioClip::new(vec![0.3; 16_000]),
                starts: recordings.clone(),
            }),
            sink: sink.clone(),
            frontmost: Arc::new(FakeFront),
            focus_warmer: Arc::new(NoopFocusWarmer),
            music_mute: Arc::new(music_mute.clone()),
            selection: Arc::new(FakeSelectionSource {
                text: options.selection,
                surrounding: options.surrounding,
                later: options.later_selection,
                editable: options.editable,
                later_editable: options.later_editable,
                replaced: replaced.clone(),
                captures: captures.clone(),
                readings: options.selection_reads,
                gate: options.selection_gate,
            }),
        },
        config: config.clone(),
        store: store.clone(),
        engines: Arc::new(EndlessWarmUp { engines, warm_ups: warm_ups.clone() }),
        audio_dir: Some(audio_dir),
    });
    Rig {
        h,
        sink,
        replaced,
        store,
        config,
        asr,
        router,
        warm_ups,
        mute: music_mute,
        recordings,
        captures,
        gate: gate_control,
        _dir: dir,
    }
}

fn simple_rig(text: &str) -> Rig {
    rig(MockTranscriber::new(text), MockTextModel::default(), RigOptions::default())
}

fn failing_rig(kind: ErrorKind) -> Rig {
    rig(MockTranscriber::new("x").script([Err(kind)]), MockTextModel::default(), RigOptions::default())
}

async fn wait_for(h: &DictationHandle, want: impl Fn(&HudState) -> bool) -> HudState {
    let mut rx = h.hud();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let cur = rx.borrow_and_update().clone();
            if want(&cur) {
                return cur;
            }
            rx.changed().await.unwrap();
        }
    })
    .await
    .expect("timed out waiting for HUD state")
}

/// History is written on the blocking pool, so a row appears shortly after the HUD state it belongs to.
async fn history_of(store: &Store, count: usize) -> Vec<crate::store::HistoryEntry> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let rows = store.list_history(&HistoryFilter::default()).unwrap();
        if rows.len() >= count {
            return rows;
        }
        assert!(std::time::Instant::now() < deadline, "history row was not written");
        tokio::task::yield_now().await;
    }
}

fn default_labels() -> EngineLabels {
    EngineLabels {
        asr: Some(format!("local:{DEFAULT_ASR_LOCAL}")),
        polish: Some(format!("local:{DEFAULT_TEXT_LOCAL}")),
    }
}

#[tokio::test(start_paused = true)]
async fn recording_limit_finishes_both_modes_and_ignores_old_timers() {
    for mode in [RecordMode::Toggle, RecordMode::Hold] {
        let r = rig(
            MockTranscriber::new("hello").with_delay(Duration::from_secs(30)),
            MockTextModel::default(),
            RigOptions { mode, ..Default::default() },
        );
        r.h.send(Command::KeyDown);
        wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
        assert!(r.h.recording_started().is_some());
        // Let the scheduled timer start before advancing virtual time.
        tokio::task::yield_now().await;
        tokio::time::advance(MAX_RECORDING_DURATION - Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        assert!(matches!(*r.h.hud().borrow(), HudState::Listening { .. }));
        tokio::time::advance(Duration::from_secs(1)).await;
        wait_for(&r.h, |s| *s == HudState::Transcribing).await;
        assert!(r.h.recording_started().is_none());
    }

    let r = simple_rig("hello");
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    r.h.send(Command::Cancel);
    wait_for(&r.h, |s| *s == HudState::Hidden).await;
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    r.h.tx.send(Msg::RecordingLimit(1)).unwrap();
    tokio::task::yield_now().await;
    assert!(matches!(*r.h.hud().borrow(), HudState::Listening { .. }));
}

#[tokio::test(start_paused = true)]
async fn toggle_mode_runs_listen_transcribe_polish_insert_then_hides() {
    let r = simple_rig("えーと 会議です");
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { mode: RecordMode::Toggle })).await;
    r.h.send(Command::KeyUp); // releasing the key must not end a toggle recording
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| *s == HudState::Inserted).await;
    assert_eq!(*r.sink.delivered.lock().unwrap(), ["会議です。"]);
    wait_for(&r.h, |s| *s == HudState::Hidden).await;

    let h = history_of(&r.store, 1).await;
    assert_eq!(h.len(), 1);
    let slack = AppTarget::builtin("slack").unwrap();
    assert_eq!((h[0].app.as_str(), h[0].style.clone()), ("Slack", slack), "style follows the frontmost app");
    assert_eq!(h[0].raw, "えーと 会議です");
    assert!(h[0].audio_path.as_deref().is_some_and(|p| std::path::Path::new(p).exists()));
    assert_eq!(h[0].engines, default_labels(), "the row names the engines of the settings it ran with");
}

#[tokio::test]
async fn an_unwritable_audio_folder_never_fails_the_dictation() {
    let r = rig(
        MockTranscriber::new("hello"),
        MockTextModel::default(),
        RigOptions { audio_dir_blocked: true, ..Default::default() },
    );
    r.h.send(Command::KeyDown);
    r.h.send(Command::Finish);
    wait_for(&r.h, |s| *s == HudState::Inserted).await;
    let h = history_of(&r.store, 1).await;
    assert_eq!(h[0].kind, HistoryKind::Dictation);
    assert_eq!(h[0].audio_path, None);
}

#[tokio::test]
async fn hold_mode_ends_on_key_up_and_ignores_a_second_key_down() {
    let r = rig(
        MockTranscriber::new("hi"),
        MockTextModel::default(),
        RigOptions { mode: RecordMode::Hold, ..Default::default() },
    );
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { mode: RecordMode::Hold })).await;
    r.h.send(Command::KeyDown);
    r.h.send(Command::KeyUp);
    wait_for(&r.h, |s| *s == HudState::Inserted).await;
    assert_eq!(r.sink.delivered.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn cancelling_while_listening_keeps_the_audio_as_a_cancelled_entry() {
    let r = simple_rig("x");
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    r.h.send(Command::Cancel);
    wait_for(&r.h, |s| *s == HudState::Hidden).await;
    assert!(r.sink.delivered.lock().unwrap().is_empty());
    assert_eq!(r.asr.calls(), 0, "cancel must not recognize");
    let h = history_of(&r.store, 1).await;
    assert_eq!(h[0].kind, HistoryKind::Cancelled);
    assert!(h[0].audio_path.is_some(), "the audio is what 再認識 needs");
    assert_eq!(h[0].engines, EngineLabels::default(), "no engine had run, so there is nothing to name");
}

#[tokio::test(start_paused = true)]
async fn the_engine_warm_up_skips_a_typing_chord_and_ends_with_its_recording() {
    let r = simple_rig("会議です");
    let warm_ups = |count: &AtomicUsize| count.load(Ordering::SeqCst);

    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    r.h.send(Command::ChordAbort);
    wait_for(&r.h, |s| *s == HudState::Hidden).await;
    tokio::time::sleep(WARM_UP_DELAY * 2).await;
    assert_eq!(warm_ups(&r.warm_ups.started), 0, "typing warms nothing up");

    for (end, ended) in [(Command::Cancel, HudState::Hidden), (Command::Finish, HudState::Inserted)] {
        r.h.send(Command::KeyDown);
        wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
        tokio::time::sleep(WARM_UP_DELAY * 2).await;
        assert_eq!(warm_ups(&r.warm_ups.running), 1, "the warm-up runs while the user speaks");
        r.h.send(end);
        wait_for(&r.h, |s| *s == ended).await;
        settle().await;
        assert_eq!(warm_ups(&r.warm_ups.running), 0, "nothing is left running after {ended:?}");
    }
    assert_eq!(warm_ups(&r.warm_ups.started), 2);
    assert_eq!(*r.sink.delivered.lock().unwrap(), ["会議です。"]);
}

#[tokio::test]
async fn stop_during_processing_cancels_and_records_it() {
    let asr = MockTranscriber::new("x").with_delay(Duration::from_secs(30));
    let r = rig(asr, MockTextModel::default(), RigOptions::default());
    r.h.send(Command::KeyDown);
    r.h.send(Command::Finish);
    wait_for(&r.h, |s| *s == HudState::Transcribing).await;
    r.h.send(Command::Stop);
    wait_for(&r.h, |s| *s == HudState::Hidden).await;
    assert!(r.sink.delivered.lock().unwrap().is_empty());
    let h = history_of(&r.store, 1).await;
    assert_eq!(h[0].kind, HistoryKind::Cancelled);
    assert_eq!(h[0].engines, default_labels(), "a run cancelled while processing still names its engines");
}

#[tokio::test]
async fn auth_failure_shows_error_records_history_and_retry_succeeds() {
    let r = rig(
        MockTranscriber::new("やり直し").script([Err(ErrorKind::Auth)]),
        MockTextModel::default(),
        RigOptions::default(),
    );
    r.h.send(Command::KeyDown);
    r.h.send(Command::Finish);
    let s = wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
    assert!(matches!(s, HudState::Error { kind: ErrorKind::Auth, action: ErrorAction::OpenSettings, .. }));
    let failed = history_of(&r.store, 1).await;
    assert_eq!(failed[0].kind, HistoryKind::Failed);
    assert_eq!(failed[0].engines, default_labels(), "a failed run still names the engines it used");

    // A retry takes a new snapshot of the settings, so its row names the engines as they are by then.
    r.config.update(|c| c.asr.local_model = "retry-asr".into()).unwrap();
    r.h.send(Command::Retry);
    wait_for(&r.h, |s| *s == HudState::Inserted).await;
    assert_eq!(*r.sink.delivered.lock().unwrap(), ["やり直し。"]);
    let rows = history_of(&r.store, 2).await;
    assert_eq!(rows[0].kind, HistoryKind::Dictation, "the retry's row is the newer one");
    assert_eq!(rows[0].engines.asr.as_deref(), Some("local:retry-asr"));
}

#[tokio::test]
async fn microphone_failure_offers_open_settings() {
    let r =
        rig(MockTranscriber::new("x"), MockTextModel::default(), RigOptions { mic_fails: true, ..Default::default() });
    r.h.send(Command::KeyDown);
    let s = wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
    assert!(matches!(s, HudState::Error { kind: ErrorKind::Mic, action: ErrorAction::OpenSettings, .. }));
}

#[tokio::test]
async fn no_insertion_target_reports_copied() {
    let r = rig(
        MockTranscriber::new("hello"),
        MockTextModel::default(),
        RigOptions { no_insertion_target: true, ..Default::default() },
    );
    r.h.send(Command::KeyDown);
    r.h.send(Command::Finish);
    wait_for(&r.h, |s| *s == HudState::Copied).await;
}

#[tokio::test]
async fn a_key_press_during_delivery_starts_listening_only_after_its_result() {
    let mut r = rig(
        MockTranscriber::new("会議です"),
        MockTextModel::default(),
        RigOptions { gated_delivery: true, ..Default::default() },
    );
    r.h.send(Command::KeyDown);
    r.h.send(Command::Finish);
    r.delivery_started().await;
    assert_eq!(history_of(&r.store, 1).await[0].kind, HistoryKind::Dictation);
    r.h.send(Command::KeyDown);
    r.h.send(Command::KeyUp); // releasing a toggle shortcut must not take the press back
    tokio::task::yield_now().await;
    assert!(matches!(*r.h.hud().borrow(), HudState::Transcribing | HudState::Polishing));
    assert!(r.h.recording_started().is_none(), "the press waits for the first dictation to finish");

    r.release_delivery();
    wait_for(&r.h, |s| matches!(s, HudState::Listening { mode: RecordMode::Toggle })).await;
    assert!(r.h.recording_started().is_some());
    assert_eq!(*r.sink.delivered.lock().unwrap(), ["会議です。"], "the first dictation is typed exactly once");
}

#[tokio::test]
async fn in_hold_mode_releasing_the_key_during_delivery_drops_the_held_back_press() {
    let mut r = rig(
        MockTranscriber::new("会議です"),
        MockTextModel::default(),
        RigOptions { mode: RecordMode::Hold, gated_delivery: true, ..Default::default() },
    );
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { mode: RecordMode::Hold })).await;
    r.h.send(Command::KeyUp);
    r.delivery_started().await;
    r.h.send(Command::KeyDown);
    r.h.send(Command::KeyUp);
    tokio::task::yield_now().await;

    r.release_delivery();
    wait_for(&r.h, |s| *s == HudState::Inserted).await;
    assert!(r.h.recording_started().is_none(), "a hold recording for a finished press would never end");
}

#[tokio::test(start_paused = true)]
async fn commands_during_delivery_cannot_change_its_outcome() {
    let mut r = rig(
        MockTranscriber::new("会議です"),
        MockTextModel::default(),
        RigOptions { gated_delivery: true, ..Default::default() },
    );
    r.h.send(Command::KeyDown);
    r.h.send(Command::Finish);
    r.delivery_started().await;
    for c in [
        Command::Cancel,
        Command::Stop,
        Command::Finish,
        Command::Retry,
        Command::Dismiss,
        Command::ErrorHover(true),
        Command::KeyUp,
    ] {
        r.h.send(c);
    }
    tokio::task::yield_now().await;
    assert!(matches!(*r.h.hud().borrow(), HudState::Transcribing | HudState::Polishing));

    r.release_delivery();
    wait_for(&r.h, |s| *s == HudState::Inserted).await;
    assert_eq!(*r.h.hud().borrow(), HudState::Inserted);
    tokio::time::advance(SUCCESS_FLASH + Duration::from_millis(10)).await;
    wait_for(&r.h, |s| *s == HudState::Hidden).await;
    assert_eq!(*r.sink.delivered.lock().unwrap(), ["会議です。"], "typed once, never retried");
    let rows = history_of(&r.store, 1).await;
    assert_eq!(rows.iter().map(|e| e.kind).collect::<Vec<_>>(), [HistoryKind::Dictation]);
}

#[tokio::test]
async fn no_speech_is_shown_but_not_recorded_as_a_failure() {
    let r = failing_rig(ErrorKind::NoSpeech);
    r.h.send(Command::KeyDown);
    r.h.send(Command::Finish);
    let s = wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
    assert!(matches!(s, HudState::Error { kind: ErrorKind::NoSpeech, .. }));
    r.h.send(Command::Retry);
    wait_for(&r.h, |s| *s == HudState::Inserted).await;
    let rows = history_of(&r.store, 1).await;
    assert_eq!(rows.iter().map(|e| e.kind).collect::<Vec<_>>(), [HistoryKind::Dictation]);
}

#[tokio::test(start_paused = true)]
async fn an_error_closes_after_its_linger_time() {
    let r = failing_rig(ErrorKind::Network);
    r.h.send(Command::KeyDown);
    r.h.send(Command::Finish);
    wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
    tokio::time::advance(ERROR_LINGER - Duration::from_millis(10)).await;
    tokio::task::yield_now().await;
    assert!(matches!(*r.h.hud().borrow(), HudState::Error { .. }));
    tokio::time::advance(Duration::from_millis(20)).await;
    wait_for(&r.h, |s| *s == HudState::Hidden).await;
}

#[tokio::test(start_paused = true)]
async fn an_old_error_timeout_cannot_hide_a_new_recording() {
    let r = failing_rig(ErrorKind::Network);
    r.h.send(Command::KeyDown);
    r.h.send(Command::Finish);
    wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
    tokio::time::advance(ERROR_LINGER * 2 / 3).await;
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    tokio::time::advance(ERROR_LINGER * 2 / 3).await;
    tokio::task::yield_now().await;
    assert!(matches!(*r.h.hud().borrow(), HudState::Listening { .. }));
    r.h.send(Command::Cancel);
}

#[tokio::test(start_paused = true)]
async fn hovering_an_error_cancels_and_restarts_its_full_lifetime() {
    let r = failing_rig(ErrorKind::Network);
    r.h.send(Command::KeyDown);
    r.h.send(Command::Finish);
    wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
    tokio::time::advance(ERROR_LINGER / 3).await;
    r.h.send(Command::ErrorHover(true));
    tokio::task::yield_now().await;
    tokio::time::advance(ERROR_LINGER * 3).await;
    tokio::task::yield_now().await;
    assert!(matches!(*r.h.hud().borrow(), HudState::Error { .. }));
    r.h.send(Command::ErrorHover(false));
    tokio::task::yield_now().await;
    tokio::time::advance(ERROR_LINGER - Duration::from_millis(100)).await;
    tokio::task::yield_now().await;
    assert!(matches!(*r.h.hud().borrow(), HudState::Error { .. }));
    tokio::time::advance(Duration::from_millis(200)).await;
    wait_for(&r.h, |s| *s == HudState::Hidden).await;
}

#[tokio::test]
async fn auto_mode_mutes_while_listening_and_restores_when_recording_ends() {
    let r = rig(
        MockTranscriber::new("x"),
        MockTextModel::default(),
        RigOptions { music_mute: MusicMuteMode::Auto, ..Default::default() },
    );
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    assert_eq!(r.mute.log(), ["mute"], "music is muted for the whole listening phase");
    r.h.send(Command::Finish);
    // Transcription and insertion may still be running; the microphone is done, so music returns.
    wait_for(&r.h, |s| *s == HudState::Inserted).await;
    assert_eq!(r.mute.log(), ["mute", "unmute"]);
}

#[tokio::test]
async fn auto_mode_restores_music_when_listening_is_cancelled() {
    let r = rig(
        MockTranscriber::new("x"),
        MockTextModel::default(),
        RigOptions { music_mute: MusicMuteMode::Auto, ..Default::default() },
    );
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    r.h.send(Command::Cancel);
    wait_for(&r.h, |s| *s == HudState::Hidden).await;
    assert_eq!(r.mute.log(), ["mute", "unmute"], "cancel must not leave the music muted");
}

#[tokio::test]
async fn off_mode_never_mutes() {
    let r = simple_rig("x");
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    r.h.send(Command::Finish);
    wait_for(&r.h, |s| *s == HudState::Inserted).await;
    assert!(r.mute.log().is_empty(), "off must leave other audio untouched");
}

#[tokio::test]
async fn auto_mode_with_mic_failure_never_mutes() {
    let r = rig(
        MockTranscriber::new("x"),
        MockTextModel::default(),
        RigOptions { mic_fails: true, music_mute: MusicMuteMode::Auto, ..Default::default() },
    );
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
    assert!(r.mute.log().is_empty(), "no recording means nothing to protect");
}

fn command_rig(selection: Option<&'static str>, router: MockRouter, result: &str) -> Rig {
    rig(
        MockTranscriber::new("make it shorter"),
        MockTextModel::default().script([Ok(result.to_string())]),
        RigOptions { selection, router, ..Default::default() },
    )
}

fn routes_to(intent: Intent, confidence: f32) -> MockRouter {
    MockRouter::default().script([Ok(decision(intent, confidence))])
}

/// Presses the command shortcut twice: toggle mode starts the recording, then ends it.
async fn speak_command(r: &Rig) {
    r.h.send(Command::CommandKeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    r.h.send(Command::CommandKeyDown);
}

async fn wait_until(mut ready: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !ready() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("timed out waiting for condition");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_command_starts_recording_while_its_first_selection_is_blocked() {
    let (gate, mut control) = selection_gate(&[1]);
    let r = rig(
        MockTranscriber::new("make it shorter"),
        MockTextModel::default(),
        RigOptions { selection_gate: Some(gate), selection: Some("text"), ..Default::default() },
    );

    r.h.send(Command::CommandKeyDown);
    control.entered(1).await;
    assert_eq!(r.recordings.load(Ordering::SeqCst), 1);
    assert!(r.h.recording_started().is_none(), "the actor is still completing the initial capture");

    control.release(1);
    control.finished(1).await;
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
}

#[tokio::test]
async fn command_transcription_and_routing_wait_only_for_their_required_selection() {
    let (gate, mut control) = selection_gate(&[2]);
    let spy = Arc::new(SpyTextModel::default());
    let r = rig(
        MockTranscriber::new("make it shorter"),
        spy.clone(),
        RigOptions {
            selection_reads: Some(vec![Some("first"), Some("second")]),
            selection_gate: Some(gate),
            router: routes_to(Intent::Rewrite, 0.99),
            ..Default::default()
        },
    );

    speak_command(&r).await;
    control.entered(2).await;
    wait_until(|| r.asr.calls() == 1).await;
    assert_eq!(r.router.calls(), 0, "routing waits for the second selection");
    assert_eq!(*r.h.hud().borrow(), HudState::Transcribing);

    control.release(2);
    control.finished(2).await;
    wait_for(&r.h, |s| *s == HudState::Replaced).await;
    assert!(spy.seen.lock().unwrap()[0].1.contains("===\nsecond\n==="));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_blocked_selection_keeps_the_next_command_clean() {
    let (gate, mut control) = selection_gate(&[2]);
    let spy = Arc::new(SpyTextModel::default());
    let r = rig(
        MockTranscriber::new("make it shorter"),
        spy.clone(),
        RigOptions {
            selection_reads: Some(vec![Some("old first"), Some("old second"), Some("new first"), Some("new second")]),
            selection_gate: Some(gate),
            router: routes_to(Intent::Rewrite, 0.99),
            ..Default::default()
        },
    );

    speak_command(&r).await;
    control.entered(2).await;
    wait_until(|| r.asr.calls() == 1).await;
    r.h.send(Command::Cancel);
    wait_for(&r.h, |s| *s == HudState::Hidden).await;

    speak_command(&r).await;
    control.release(2);
    control.finished(2).await;
    wait_for(&r.h, |s| *s == HudState::Replaced).await;
    assert!(spy.seen.lock().unwrap()[0].1.contains("===\nnew second\n==="));
    tokio::task::yield_now().await;
    assert!(!spy.seen.lock().unwrap()[0].1.contains("old second"));
}

#[tokio::test]
async fn a_confident_intent_replaces_the_selection_and_records_the_command() {
    let r = command_rig(Some("teh text"), routes_to(Intent::Polish, 0.97), "the text");
    r.h.send(Command::CommandKeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    assert_eq!(r.h.run_kind(), RunKind::Command);
    r.h.send(Command::CommandKeyDown);
    wait_for(&r.h, |s| *s == HudState::Replaced).await;
    assert_eq!(*r.replaced.lock().unwrap(), ["the text"]);
    assert!(r.sink.delivered.lock().unwrap().is_empty());
    wait_for(&r.h, |s| *s == HudState::Hidden).await;

    let rows = history_of(&r.store, 1).await;
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.kind, HistoryKind::Command);
    assert_eq!((row.raw.as_str(), row.polished.as_deref()), ("make it shorter", Some("the text")));
    assert_eq!(row.source.as_deref(), Some("teh text"));
    assert_eq!((row.app.as_str(), row.duration_ms, row.audio_path.as_deref()), ("Slack", 1_000, None));
    assert_eq!(row.engines, default_labels());
    assert!(row.timings.asr.is_some() && row.timings.polish.is_some());
    let details = row.command.as_ref().expect("a command row has its details");
    assert_eq!(details.origin, Some(InstructionOrigin::Spoken));
    let decision = details.decision.as_ref().expect("the decision is kept");
    assert_eq!((decision.intent, decision.confidence), (Intent::Polish, 0.97));
    assert_eq!(details.choice, Some(ToolChoice::Confident));
    assert_eq!((details.tool, details.source), (Some(Intent::Polish), Some(SourceKind::Selection)));
    assert_eq!(details.outcome, CommandOutcome::Replaced);
    assert!(details.route_ms.is_some());
    assert_eq!(details.router_engine, Some(format!("local:{DEFAULT_TEXT_LOCAL}")));
}

#[tokio::test]
async fn a_typed_instruction_chosen_by_the_user_is_recorded_as_such() {
    let unsure = Decision {
        intent: Intent::Rewrite,
        confidence: 0.5,
        probabilities: [0.3, 0.5, 0.1, 0.03, 0.02, 0.02, 0.02, 0.01],
        rewrite_op: None,
    };
    let r = command_rig(Some("a text"), MockRouter::default().script([Ok(unsure)]), "translated");
    r.h.send(Command::CommandKeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    r.h.send(Command::TypeInstruction);
    wait_for(&r.h, |s| *s == HudState::TypingInstruction).await;
    r.h.send(Command::SubmitInstruction("  to English ".into()));
    wait_for(&r.h, |s| matches!(s, HudState::Choosing { .. })).await;
    r.h.send(Command::Choose(Intent::Translate));
    wait_for(&r.h, |s| *s == HudState::Replaced).await;

    let rows = history_of(&r.store, 1).await;
    let (row, details) = (&rows[0], rows[0].command.as_ref().unwrap());
    assert_eq!((row.raw.as_str(), row.duration_ms, row.timings.asr), ("to English", 0, None));
    assert_eq!(row.engines.asr, None, "no speech recognition ran");
    assert_eq!(details.origin, Some(InstructionOrigin::Typed));
    assert_eq!(details.decision.as_ref().map(|d| d.intent), Some(Intent::Rewrite));
    assert_eq!((details.choice, details.tool), (Some(ToolChoice::UserChoice), Some(Intent::Translate)));
}

#[tokio::test]
async fn the_polish_button_run_has_no_instruction_or_decision() {
    let r = command_rig(Some("teh text"), MockRouter::default(), "the text");
    r.h.send(Command::CommandKeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    r.h.send(Command::PolishSelection);
    wait_for(&r.h, |s| *s == HudState::Replaced).await;

    let row = history_of(&r.store, 1).await.remove(0);
    let details = row.command.as_ref().unwrap();
    assert_eq!(row.raw, "");
    assert_eq!((details.origin, details.decision.as_ref(), details.route_ms), (None, None, None));
    assert_eq!((details.choice, details.outcome), (Some(ToolChoice::PolishButton), CommandOutcome::Replaced));
}

#[tokio::test]
async fn entered_text_an_answer_and_a_closed_window_each_end_in_their_own_row() {
    let r = command_rig(None, routes_to(Intent::Translate, 0.99), "hallo");
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::NeedsText).await;
    r.h.send(Command::SubmitText("hello".into()));
    wait_for(&r.h, |s| *s == HudState::Inserted).await;
    let row = history_of(&r.store, 1).await.remove(0);
    let details = row.command.as_ref().unwrap();
    assert_eq!((row.source.as_deref(), details.source), (Some("hello"), Some(SourceKind::Entered)));
    assert_eq!(details.outcome, CommandOutcome::Inserted);

    let r = command_rig(None, routes_to(Intent::Ask, 0.99), "It is four.");
    speak_command(&r).await;
    wait_for(&r.h, |s| matches!(s, HudState::Answer { .. })).await;
    r.h.send(Command::Dismiss);
    wait_for(&r.h, |s| *s == HudState::Hidden).await;
    let row = history_of(&r.store, 1).await.remove(0);
    let details = row.command.as_ref().unwrap();
    assert_eq!((row.polished.as_deref(), row.source.as_deref()), (Some("It is four."), None));
    assert_eq!((details.tool, details.outcome), (Some(Intent::Ask), CommandOutcome::Answered));

    let r = command_rig(Some("a long text"), routes_to(Intent::Summarize, 0.99), "short");
    speak_command(&r).await;
    wait_for(&r.h, |s| matches!(s, HudState::Confirm { .. })).await;
    r.h.send(Command::Dismiss);
    let row = history_of(&r.store, 1).await.remove(0);
    assert_eq!(row.command.as_ref().unwrap().outcome, CommandOutcome::Closed);
    assert_eq!(row.polished.as_deref(), Some("short"), "the result the window showed is kept");
}

#[tokio::test]
async fn a_failed_tool_and_a_cancel_intent_are_recorded() {
    let r = rig(
        MockTranscriber::new("make it shorter"),
        MockTextModel::default().script([Err(ErrorKind::Network)]),
        RigOptions { selection: Some("a text"), router: routes_to(Intent::Rewrite, 0.99), ..Default::default() },
    );
    speak_command(&r).await;
    wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
    let row = history_of(&r.store, 1).await.remove(0);
    assert_eq!(row.command.as_ref().unwrap().outcome, CommandOutcome::Failed);
    assert!(row.error.as_deref().is_some_and(|e| e.contains("Network")), "{:?}", row.error);
    assert_eq!(row.polished, None);

    let r = command_rig(Some("a text"), routes_to(Intent::Cancel, 0.98), "unused");
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::Hidden).await;
    let row = history_of(&r.store, 1).await.remove(0);
    let details = row.command.as_ref().unwrap();
    assert_eq!((details.outcome, details.tool), (CommandOutcome::Cancelled, None));
    assert_eq!(row.source.as_deref(), Some("a text"), "the selection is kept even when no tool ran");
}

#[tokio::test]
async fn low_confidence_offers_the_ranked_choices_and_runs_the_chosen_one() {
    let unsure = Decision {
        intent: Intent::Rewrite,
        confidence: 0.5,
        probabilities: [0.3, 0.5, 0.1, 0.03, 0.02, 0.02, 0.02, 0.01],
        rewrite_op: None,
    };
    let r = command_rig(Some("a text"), MockRouter::default().script([Ok(unsure)]), "translated");
    speak_command(&r).await;
    let HudState::Choosing { choices } = wait_for(&r.h, |s| matches!(s, HudState::Choosing { .. })).await else {
        unreachable!()
    };
    let shown: Vec<_> = choices.iter().map(|c| (c.intent, c.percent)).collect();
    assert_eq!(shown, [(Intent::Rewrite, 50), (Intent::Polish, 30), (Intent::Translate, 10)], "never cancel");

    // Another press of either shortcut must not take over a window that is waiting for a choice.
    r.h.send(Command::KeyDown);
    r.h.send(Command::CommandKeyDown);
    r.h.send(Command::Choose(Intent::Translate));
    wait_for(&r.h, |s| *s == HudState::Replaced).await;
    assert_eq!(*r.replaced.lock().unwrap(), ["translated"]);
}

#[tokio::test]
async fn without_a_selection_the_tool_asks_for_text_and_inserts_the_result() {
    let r = command_rig(None, routes_to(Intent::Translate, 0.99), "hallo");
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::NeedsText).await;
    r.h.send(Command::SubmitText("hello".into()));
    wait_for(&r.h, |s| *s == HudState::Inserted).await;
    assert_eq!(*r.sink.delivered.lock().unwrap(), ["hallo"]);
    assert!(r.replaced.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_summary_waits_in_a_window_and_replaces_with_the_edited_text() {
    let r = command_rig(Some("a long text"), routes_to(Intent::Summarize, 0.99), "short");
    speak_command(&r).await;
    let confirm = HudState::Confirm { text: "short".into(), write: ResultWrite::ReplaceSelection };
    wait_for(&r.h, |s| *s == confirm).await;
    r.h.send(Command::Replace("short, edited".into()));
    wait_for(&r.h, |s| *s == HudState::Replaced).await;
    assert_eq!(*r.replaced.lock().unwrap(), ["short, edited"]);
}

#[tokio::test(start_paused = true)]
async fn a_window_never_hides_on_its_own_and_dismiss_closes_it() {
    let r = command_rig(Some("a long text"), routes_to(Intent::Summarize, 0.99), "short");
    speak_command(&r).await;
    wait_for(&r.h, |s| matches!(s, HudState::Confirm { .. })).await;
    tokio::time::advance(Duration::from_secs(600)).await;
    tokio::task::yield_now().await;
    assert!(matches!(*r.h.hud().borrow(), HudState::Confirm { .. }));
    r.h.send(Command::Dismiss);
    wait_for(&r.h, |s| *s == HudState::Hidden).await;
    assert!(r.replaced.lock().unwrap().is_empty());
}

#[tokio::test]
async fn the_cancel_intent_closes_the_capsule_and_changes_nothing() {
    let r = command_rig(Some("a text"), routes_to(Intent::Cancel, 0.98), "unused");
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::Hidden).await;
    assert!(r.replaced.lock().unwrap().is_empty() && r.sink.delivered.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_question_is_answered_in_a_window_and_copy_uses_the_clipboard() {
    let r = command_rig(None, routes_to(Intent::Ask, 0.99), "It is four.");
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::Answer { text: "It is four.".into() }).await;
    r.h.send(Command::CopyResult("It is four.".into()));
    wait_for(&r.h, |s| *s == HudState::Copied).await;
    assert_eq!(*r.sink.copied.lock().unwrap(), ["It is four."]);
    assert!(r.sink.delivered.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_reply_waits_in_a_copy_only_window_and_never_writes() {
    let r = command_rig(Some("Can you join on Friday?"), routes_to(Intent::Reply, 0.99), "Yes, Friday works.");
    speak_command(&r).await;
    let confirm = HudState::Confirm { text: "Yes, Friday works.".into(), write: ResultWrite::CopyOnly };
    wait_for(&r.h, |s| *s == confirm).await;

    r.h.send(Command::Replace("Yes, Friday works for me.".into()));
    settle().await;
    assert_eq!(*r.h.hud().borrow(), confirm, "Replace does nothing for a reply");

    r.h.send(Command::CopyResult("Yes, Friday works for me.".into()));
    wait_for(&r.h, |s| *s == HudState::Copied).await;
    assert_eq!(*r.sink.copied.lock().unwrap(), ["Yes, Friday works for me."]);
    assert!(r.sink.delivered.lock().unwrap().is_empty() && r.replaced.lock().unwrap().is_empty());

    let row = history_of(&r.store, 1).await.remove(0);
    let details = row.command.as_ref().unwrap();
    assert_eq!((details.tool, details.outcome), (Some(Intent::Reply), CommandOutcome::Copied));
    assert_eq!((row.source.as_deref(), details.source), (Some("Can you join on Friday?"), Some(SourceKind::Selection)));
}

/// A command rig whose selection (a preview, a web page) cannot be written over.
fn read_only_rig(intent: Intent, result: &str) -> Rig {
    let options = RigOptions {
        selection: Some("rendered text"),
        editable: false,
        later_editable: false,
        router: routes_to(intent, 0.99),
        ..Default::default()
    };
    rig(MockTranscriber::new("make it shorter"), MockTextModel::default().script([Ok(result.to_string())]), options)
}

#[tokio::test]
async fn a_tool_over_a_selection_that_cannot_be_edited_waits_in_a_copy_only_window_and_never_writes() {
    // Whatever the confirm setting says, and for every tool that would replace the selection.
    for (intent, confirm_before_replace) in
        [(Intent::Polish, false), (Intent::Rewrite, true), (Intent::Translate, false), (Intent::Summarize, false)]
    {
        let r = read_only_rig(intent, "made");
        r.config.update(|c| c.general.confirm_before_replace = confirm_before_replace).unwrap();
        speak_command(&r).await;
        let confirm = HudState::Confirm { text: "made".into(), write: ResultWrite::CopyOnly };
        wait_for(&r.h, |s| *s == confirm).await;

        r.h.send(Command::Replace("made, edited".into()));
        settle().await;
        assert_eq!(*r.h.hud().borrow(), confirm, "{intent:?}: Replace does nothing");

        r.h.send(Command::CopyResult("made, edited".into()));
        wait_for(&r.h, |s| *s == HudState::Copied).await;
        assert_eq!(*r.sink.copied.lock().unwrap(), ["made, edited"]);
        assert_nothing_ran(&r);
        let row = history_of(&r.store, 1).await.remove(0);
        let details = row.command.as_ref().unwrap();
        assert_eq!((details.tool, details.outcome), (Some(intent), CommandOutcome::Copied));
        assert_eq!(row.source.as_deref(), Some("rendered text"));
    }
}

#[tokio::test]
async fn a_question_about_a_selection_that_cannot_be_edited_is_still_answered_in_its_window() {
    let r = read_only_rig(Intent::Ask, "It is four.");
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::Answer { text: "It is four.".into() }).await;
}

#[tokio::test]
async fn text_entered_in_the_window_is_inserted_even_if_the_selection_could_not_be_edited() {
    let options = RigOptions {
        surrounding: around(),
        editable: false,
        router: routes_to(Intent::Translate, 0.99),
        ..Default::default()
    };
    let r = rig(MockTranscriber::new("translate"), MockTextModel::default().script([Ok("hallo".into())]), options);
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::NeedsText).await;
    r.h.send(Command::SubmitText("hello".into()));
    wait_for(&r.h, |s| *s == HudState::Inserted).await;
    assert_eq!(*r.sink.delivered.lock().unwrap(), ["hallo"]);
}

#[tokio::test]
async fn the_second_reading_decides_whether_the_selection_can_be_written_over() {
    // The user clicked from the preview into the editor while speaking.
    let options = RigOptions {
        selection: Some("rendered text"),
        editable: false,
        later_selection: Some(Some("typed text")),
        later_editable: true,
        router: routes_to(Intent::Rewrite, 0.99),
        ..Default::default()
    };
    let r = rig(MockTranscriber::new("make it shorter"), MockTextModel::default().script([Ok("made".into())]), options);
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::Replaced).await;
    assert_eq!(*r.replaced.lock().unwrap(), ["made"]);

    // And the other way round.
    let options = RigOptions {
        selection: Some("typed text"),
        later_selection: Some(Some("rendered text")),
        later_editable: false,
        router: routes_to(Intent::Rewrite, 0.99),
        ..Default::default()
    };
    let r = rig(MockTranscriber::new("make it shorter"), MockTextModel::default().script([Ok("made".into())]), options);
    speak_command(&r).await;
    let confirm = HudState::Confirm { text: "made".into(), write: ResultWrite::CopyOnly };
    wait_for(&r.h, |s| *s == confirm).await;
    assert!(r.replaced.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_confirmed_result_without_a_selection_still_inserts_at_the_cursor() {
    let r = command_rig(None, routes_to(Intent::Translate, 0.99), "hallo");
    r.config.update(|c| c.general.confirm_before_replace = true).unwrap();
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::NeedsText).await;
    r.h.send(Command::SubmitText("hello".into()));
    let confirm = HudState::Confirm { text: "hallo".into(), write: ResultWrite::InsertAtCursor };
    wait_for(&r.h, |s| *s == confirm).await;
    r.h.send(Command::Replace("hallo!".into()));
    wait_for(&r.h, |s| *s == HudState::Inserted).await;
    assert_eq!(*r.sink.delivered.lock().unwrap(), ["hallo!"]);
}

#[tokio::test]
async fn a_reply_asks_for_the_text_when_nothing_is_selected() {
    let r = command_rig(None, routes_to(Intent::Reply, 0.99), "Thanks, noted.");
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::NeedsText).await;
    r.h.send(Command::SubmitText("The invoice is attached.".into()));
    let confirm = HudState::Confirm { text: "Thanks, noted.".into(), write: ResultWrite::CopyOnly };
    wait_for(&r.h, |s| *s == confirm).await;
}

/// Nothing ran: no tool request, no insertion, no replacement.
fn assert_nothing_ran(r: &Rig) {
    assert!(r.replaced.lock().unwrap().is_empty() && r.sink.delivered.lock().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_confident_none_shows_the_notice_runs_no_tool_and_is_recorded_as_closed() {
    let r = command_rig(Some("a text"), routes_to(Intent::None, 0.98), "unused");
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::NotRecognized).await;
    assert_nothing_ran(&r);
    tokio::time::advance(ERROR_LINGER - Duration::from_millis(10)).await;
    tokio::task::yield_now().await;
    assert_eq!(*r.h.hud().borrow(), HudState::NotRecognized);
    tokio::time::advance(Duration::from_millis(20)).await;
    wait_for(&r.h, |s| *s == HudState::Hidden).await;

    let row = history_of(&r.store, 1).await.remove(0);
    let details = row.command.as_ref().unwrap();
    assert_eq!(
        (details.outcome, details.tool, details.choice),
        (CommandOutcome::Closed, None, Some(ToolChoice::Confident))
    );
    assert_eq!((row.polished.as_deref(), row.error.as_deref()), (None, None), "no result and no error");
    assert_eq!(row.source.as_deref(), Some("a text"));
}

#[tokio::test(start_paused = true)]
async fn an_unsure_decision_whose_likeliest_intent_is_none_shows_the_notice_not_the_choices() {
    let unsure = Decision {
        intent: Intent::None,
        confidence: 0.4,
        probabilities: [0.05, 0.1, 0.05, 0.05, 0.1, 0.15, 0.1, 0.4],
        rewrite_op: None,
    };
    let r = command_rig(Some("a text"), MockRouter::default().script([Ok(unsure)]), "unused");
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::NotRecognized).await;
    assert_nothing_ran(&r);
    tokio::time::advance(ERROR_LINGER + Duration::from_millis(10)).await;
    wait_for(&r.h, |s| *s == HudState::Hidden).await;
    let row = history_of(&r.store, 1).await.remove(0);
    assert_eq!(row.command.as_ref().unwrap().outcome, CommandOutcome::Closed);
}

#[tokio::test(start_paused = true)]
async fn the_notice_is_kept_by_hovering_and_a_shortcut_starts_a_new_run() {
    let r = command_rig(Some("a text"), routes_to(Intent::None, 0.98), "unused");
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::NotRecognized).await;
    r.h.send(Command::ErrorHover(true));
    tokio::task::yield_now().await;
    tokio::time::advance(ERROR_LINGER * 3).await;
    tokio::task::yield_now().await;
    assert_eq!(*r.h.hud().borrow(), HudState::NotRecognized);
    r.h.send(Command::ErrorHover(false));
    tokio::task::yield_now().await;
    tokio::time::advance(ERROR_LINGER - Duration::from_millis(100)).await;
    tokio::task::yield_now().await;
    assert_eq!(*r.h.hud().borrow(), HudState::NotRecognized, "leaving restarts the full timer");
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
}

#[tokio::test]
async fn the_choices_offer_reply_and_ask_but_never_none_or_cancel() {
    let unsure = Decision {
        intent: Intent::Reply,
        confidence: 0.4,
        probabilities: [0.05, 0.1, 0.05, 0.05, 0.4, 0.15, 0.03, 0.17],
        rewrite_op: None,
    };
    let r = command_rig(Some("a text"), MockRouter::default().script([Ok(unsure)]), "unused");
    speak_command(&r).await;
    let HudState::Choosing { choices } = wait_for(&r.h, |s| matches!(s, HudState::Choosing { .. })).await else {
        unreachable!()
    };
    let shown: Vec<_> = choices.iter().map(|c| c.intent).collect();
    assert_eq!(shown, [Intent::Reply, Intent::Ask, Intent::Rewrite]);
}

#[tokio::test]
async fn confirm_before_replace_holds_a_polish_result_and_the_polish_button_skips_routing() {
    let r = command_rig(Some("teh text"), MockRouter::default(), "the text");
    r.config.update(|c| c.general.confirm_before_replace = true).unwrap();
    r.h.send(Command::CommandKeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    r.h.send(Command::PolishSelection);
    let confirm = HudState::Confirm { text: "the text".into(), write: ResultWrite::ReplaceSelection };
    wait_for(&r.h, |s| *s == confirm).await;
    assert_eq!(r.asr.calls(), 0, "the button cancels the recording");
}

fn spy_rig(
    selection: Option<&'static str>,
    surrounding: Option<Surrounding>,
    intent: Intent,
) -> (Rig, Arc<SpyTextModel>) {
    let spy = Arc::new(SpyTextModel::default());
    let options = RigOptions { selection, surrounding, router: routes_to(intent, 0.99), ..Default::default() };
    (rig(MockTranscriber::new("make it shorter"), spy.clone(), options), spy)
}

fn around() -> Option<Surrounding> {
    Some(Surrounding::Around { before: "Dear team, ".into(), after: " Thanks, Ken.".into() })
}

#[tokio::test]
async fn a_tool_request_carries_the_text_around_the_selection_as_reference_only() {
    let (r, spy) = spy_rig(Some("the plan"), around(), Intent::Rewrite);
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::Replaced).await;

    let seen = spy.seen.lock().unwrap().clone();
    let [(system, user)] = seen.as_slice() else { panic!("one tool request expected, got {seen:?}") };
    assert!(system.contains("#Context#") && system.contains("Never rewrite it"), "{system}");
    assert_eq!(
        user,
        "<surrounding_text><before>Dear team, </before><after> Thanks, Ken.</after></surrounding_text>\n===\nthe plan\n===\n<instruction>make it shorter</instruction>"
    );
    assert_eq!(*r.replaced.lock().unwrap(), ["result"], "only the selection is replaced");
    let row = history_of(&r.store, 1).await.remove(0);
    assert_eq!(row.source.as_deref(), Some("the plan"), "the history keeps the selection, not the context");
}

#[tokio::test]
async fn field_text_without_a_selection_is_context_and_the_tool_still_asks_for_text() {
    let (r, spy) = spy_rig(None, around(), Intent::Translate);
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::NeedsText).await;
    assert!(spy.seen.lock().unwrap().is_empty(), "nothing runs before the user gives the text");
    r.h.send(Command::SubmitText("hello".into()));
    wait_for(&r.h, |s| *s == HudState::Inserted).await;

    let seen = spy.seen.lock().unwrap().clone();
    let [(system, user)] = seen.as_slice() else { panic!("one tool request expected, got {seen:?}") };
    assert!(system.contains("translate into English (the language type the user spoke"), "{system}");
    assert!(user.contains("===\nhello\n==="), "the entered text is what is worked on: {user}");
    assert!(user.contains("<before>Dear team, </before>"), "{user}");
    assert_eq!(*r.sink.delivered.lock().unwrap(), ["result"]);
    assert!(r.replaced.lock().unwrap().is_empty(), "there is no selection to replace");
    let row = history_of(&r.store, 1).await.remove(0);
    assert_eq!(row.command.as_ref().unwrap().source, Some(SourceKind::Entered));

    let (r, spy) = spy_rig(None, around(), Intent::Ask);
    speak_command(&r).await;
    wait_for(&r.h, |s| matches!(s, HudState::Answer { .. })).await;
    let seen = spy.seen.lock().unwrap().clone();
    assert!(seen[0].1.starts_with("<instruction>") && seen[0].1.ends_with("</instruction>"), "{:?}", seen[0].1);
    assert!(!seen[0].1.contains("===") && !seen[0].1.contains("surrounding_text"), "the question stands alone");
    assert!(seen[0].0.contains("No text was selected"), "{}", seen[0].0);
}

#[tokio::test]
async fn polish_works_on_the_selection_alone_and_a_long_context_is_cut() {
    let (r, spy) = spy_rig(Some("teh text"), around(), Intent::Polish);
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::Replaced).await;
    let user = spy.seen.lock().unwrap()[0].1.clone();
    assert_eq!(
        crate::prompts::unfenced(&user),
        "teh text",
        "the selection is the fenced transcript of the polish prompt"
    );

    let long = Surrounding::Around { before: "あ".repeat(100_000), after: "い".repeat(100_000) };
    let (r, spy) = spy_rig(Some("the plan"), Some(long), Intent::Rewrite);
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::Replaced).await;
    let user = spy.seen.lock().unwrap()[0].1.clone();
    assert!(user.chars().count() < 3_500, "a field of 200 000 characters stays within the budget: {}", user.len());
    assert!(user.contains("<before>…") && user.contains("…</after>"), "a cut is marked");
}

#[tokio::test]
async fn the_rewrite_op_reaches_only_a_rewrite_the_decision_chose() {
    use crate::intent::RewriteOp;
    let hinted =
        |confidence| Decision { rewrite_op: Some((RewriteOp::Casual, 0.85)), ..decision(Intent::Rewrite, confidence) };
    let run = |router: MockRouter| {
        let spy = Arc::new(SpyTextModel::default());
        let options = RigOptions { selection: Some("the plan"), router, ..Default::default() };
        (rig(MockTranscriber::new("make it friendlier"), spy.clone(), options), spy)
    };
    let hint = "make the tone more casual and friendly";

    let (r, spy) = run(MockRouter::default().script([Ok(hinted(0.95))]));
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::Replaced).await;
    assert!(spy.seen.lock().unwrap()[0].0.contains(hint), "a confident decision hints the op");

    let (r, spy) = run(MockRouter::default().script([Ok(hinted(0.5))]));
    speak_command(&r).await;
    wait_for(&r.h, |s| matches!(s, HudState::Choosing { .. })).await;
    r.h.send(Command::Choose(Intent::Rewrite));
    wait_for(&r.h, |s| *s == HudState::Replaced).await;
    assert!(!spy.seen.lock().unwrap()[0].0.contains(hint), "a choice made in the window carries no hint");
}

fn command_rig_in(mode: RecordMode, selection: Option<&'static str>, result: &str) -> Rig {
    rig(
        MockTranscriber::new("make it shorter"),
        MockTextModel::default().script([Ok(result.to_string())]),
        RigOptions { mode, selection, router: routes_to(Intent::Polish, 0.97), ..Default::default() },
    )
}

/// The actor handles messages in order, but on its own task: give it a moment before reading what it did.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(30)).await;
}

async fn wait_for_run_kind(r: &Rig, kind: RunKind) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while r.h.run_kind() != kind {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("timed out waiting for the run kind");
}

/// How often the microphone was opened and the selection was read.
fn counts(r: &Rig) -> (usize, usize) {
    (r.recordings.load(Ordering::SeqCst), r.captures.load(Ordering::SeqCst))
}

/// The messages one press of the command combination produces, by which key lands first.
const SHIFT_FIRST: &[Command] = &[Command::CommandKeyDown, Command::CommandKeyUp];
/// Right ⌘ first: the press goes to the dictation shortcut, then Shift hands it over within the chord window.
const MODIFIER_FIRST: &[Command] = &[Command::KeyDown, Command::KeyToCommand, Command::CommandKeyUp];

fn press(r: &Rig, messages: &[Command]) {
    for m in messages {
        r.h.send(m.clone());
    }
}

#[tokio::test]
async fn a_command_started_with_the_modifier_first_stays_one_run_with_one_recording() {
    let r = command_rig_in(RecordMode::Toggle, Some("teh text"), "the text");
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { mode: RecordMode::Toggle })).await;
    assert_eq!(r.h.run_kind(), RunKind::Dictation);
    let started = r.h.recording_started();
    assert_eq!(counts(&r), (1, 0), "a plain press only records");

    r.h.send(Command::KeyToCommand);
    wait_for_run_kind(&r, RunKind::Command).await;
    r.h.send(Command::CommandKeyUp);
    settle().await;
    assert_eq!(*r.h.hud().borrow(), HudState::Listening { mode: RecordMode::Toggle });
    assert_eq!(r.h.recording_started(), started, "the recording was not restarted");
    assert_eq!(counts(&r), (1, 1), "the selection is read once, at the handover");
    assert!(r.sink.delivered.lock().unwrap().is_empty());

    press(&r, MODIFIER_FIRST);
    wait_for(&r.h, |s| *s == HudState::Replaced).await;
    assert_eq!(*r.replaced.lock().unwrap(), ["the text"]);
    settle().await;
    assert_eq!(counts(&r), (1, 2), "the second press read the selection again and started no run");
    assert_eq!(r.asr.calls(), 1);
    let rows = history_of(&r.store, 1).await;
    assert_eq!((rows.len(), rows[0].kind), (1, HistoryKind::Command));
    assert_eq!(rows[0].source.as_deref(), Some("teh text"));
}

#[tokio::test]
async fn in_toggle_mode_the_command_combination_starts_and_finishes_a_command_in_either_key_order() {
    for start in [SHIFT_FIRST, MODIFIER_FIRST] {
        for finish in [SHIFT_FIRST, MODIFIER_FIRST] {
            let r = command_rig_in(RecordMode::Toggle, Some("teh text"), "the text");
            press(&r, start);
            wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
            wait_for_run_kind(&r, RunKind::Command).await;
            settle().await;
            assert_eq!(*r.h.hud().borrow(), HudState::Listening { mode: RecordMode::Toggle });
            press(&r, finish);
            // The keys that follow the finishing press, still held or released, must not cancel it or start a run.
            r.h.send(Command::ChordAbort);
            wait_for(&r.h, |s| *s == HudState::Replaced).await;
            settle().await;
            assert_eq!(*r.replaced.lock().unwrap(), ["the text"]);
            assert_eq!(counts(&r), (1, 2), "read at the start and again at the end");
            assert_eq!(r.asr.calls(), 1);
        }
    }
}

#[tokio::test]
async fn either_shortcut_finishes_a_dictation_in_toggle_mode_and_nothing_after_it_starts_another_run() {
    for finish in [&[Command::KeyDown][..], &[Command::CommandKeyDown, Command::CommandKeyUp], MODIFIER_FIRST] {
        let r = simple_rig("会議です");
        r.h.send(Command::KeyDown);
        wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
        press(&r, finish);
        r.h.send(Command::ChordAbort);
        wait_for(&r.h, |s| *s == HudState::Inserted).await;
        settle().await;
        assert_eq!(*r.sink.delivered.lock().unwrap(), ["会議です。"]);
        assert_eq!(counts(&r), (1, 0), "{finish:?}");
        assert_eq!(r.asr.calls(), 1);
    }
}

#[tokio::test]
async fn repeated_presses_alternate_between_starting_and_finishing() {
    let r = rig(
        MockTranscriber::new("make it shorter"),
        MockTextModel::default().script([Ok("one".into()), Ok("two".into())]),
        RigOptions {
            selection: Some("teh text"),
            router: MockRouter::default()
                .script([Ok(decision(Intent::Polish, 0.97)), Ok(decision(Intent::Polish, 0.97))]),
            ..Default::default()
        },
    );
    for (round, result) in ["one", "two"].into_iter().enumerate() {
        press(&r, SHIFT_FIRST);
        wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
        settle().await;
        press(&r, MODIFIER_FIRST);
        wait_for(&r.h, |s| *s == HudState::Replaced).await;
        assert_eq!(r.replaced.lock().unwrap().len(), round + 1);
        assert_eq!(r.replaced.lock().unwrap()[round], result);
        wait_for(&r.h, |s| *s == HudState::Hidden).await;
    }
    assert_eq!(counts(&r), (2, 4));
}

/// A command rig whose selection changes while the user speaks: `first` at the start, `later` from then on.
fn changing_rig(
    first: Option<&'static str>,
    later: Option<&'static str>,
    router: MockRouter,
) -> (Rig, Arc<SpyTextModel>) {
    let spy = Arc::new(SpyTextModel::default());
    let options = RigOptions {
        selection: first,
        surrounding: around(),
        later_selection: Some(later),
        router,
        ..Default::default()
    };
    (rig(MockTranscriber::new("make it shorter"), spy.clone(), options), spy)
}

#[tokio::test]
async fn the_selection_is_read_again_when_the_recording_ends_and_the_tool_gets_the_second() {
    // The selection changed while speaking: the tool, the history and the surrounding text all follow the second.
    let (r, spy) = changing_rig(Some("old text"), Some("new text"), routes_to(Intent::Rewrite, 0.99));
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::Replaced).await;
    let seen = spy.seen.lock().unwrap().clone();
    let [(_, user)] = seen.as_slice() else { panic!("one tool request expected, got {seen:?}") };
    assert!(user.contains("===\nnew text\n==="), "{user}");
    assert!(user.contains("<before>Hello, </before>") && !user.contains("Dear team"), "{user}");
    assert_eq!(counts(&r), (1, 2));
    let row = history_of(&r.store, 1).await.remove(0);
    assert_eq!(row.source.as_deref(), Some("new text"));

    // The polish button ends the recording too.
    let (r, spy) = changing_rig(Some("old text"), Some("new text"), MockRouter::default());
    r.h.send(Command::CommandKeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    r.h.send(Command::PolishSelection);
    wait_for(&r.h, |s| *s == HudState::Replaced).await;
    assert_eq!(crate::prompts::unfenced(&spy.seen.lock().unwrap()[0].1), "new text");
    assert_eq!(counts(&r), (1, 2));

    // So does the keyboard button: the typed instruction runs on the second reading.
    let (r, spy) = changing_rig(Some("old text"), Some("new text"), routes_to(Intent::Rewrite, 0.99));
    r.h.send(Command::CommandKeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    r.h.send(Command::TypeInstruction);
    wait_for(&r.h, |s| *s == HudState::TypingInstruction).await;
    r.h.send(Command::SubmitInstruction("shorter".into()));
    wait_for(&r.h, |s| *s == HudState::Replaced).await;
    assert!(spy.seen.lock().unwrap()[0].1.contains("===\nnew text\n==="));

    // A successful second reading with nothing selected replaces the first: the user deselected.
    let (r, spy) = changing_rig(Some("old text"), Some(""), routes_to(Intent::Translate, 0.99));
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::NeedsText).await;
    assert!(spy.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_failed_second_reading_keeps_the_first_selection() {
    let (r, spy) = changing_rig(Some("old text"), None, routes_to(Intent::Rewrite, 0.99));
    speak_command(&r).await;
    wait_for(&r.h, |s| *s == HudState::Replaced).await;
    let seen = spy.seen.lock().unwrap().clone();
    let [(_, user)] = seen.as_slice() else { panic!("one tool request expected, got {seen:?}") };
    assert!(user.contains("===\nold text\n===") && user.contains("<before>Dear team, </before>"), "{user}");
    assert_eq!(counts(&r), (1, 2), "the second reading was tried");
    let row = history_of(&r.store, 1).await.remove(0);
    assert_eq!(row.source.as_deref(), Some("old text"));
}

#[tokio::test]
async fn a_chord_abort_drops_the_recording_its_press_started_and_never_one_the_press_only_ended() {
    // A key right after a lone modifier that started a dictation: that was typing.
    let r = simple_rig("x");
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    r.h.send(Command::ChordAbort);
    wait_for(&r.h, |s| *s == HudState::Hidden).await;
    assert_eq!(r.asr.calls(), 0);
    assert!(r.sink.delivered.lock().unwrap().is_empty());

    // The same for a command the combination started; nothing is kept of it.
    let r = command_rig_in(RecordMode::Toggle, Some("teh text"), "the text");
    r.h.send(Command::CommandKeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    r.h.send(Command::ChordAbort);
    wait_for(&r.h, |s| *s == HudState::Hidden).await;
    settle().await;
    assert!(r.store.list_history(&HistoryFilter::default()).unwrap().is_empty());

    // A run that was listening before the press was finished by it, so the abort has nothing to drop.
    let r = rig(
        MockTranscriber::new("x").with_delay(Duration::from_secs(30)),
        MockTextModel::default(),
        RigOptions::default(),
    );
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    r.h.send(Command::KeyDown);
    r.h.send(Command::ChordAbort);
    r.h.send(Command::KeyToCommand);
    settle().await;
    assert_eq!(*r.h.hud().borrow(), HudState::Transcribing, "not cancelled, not turned into a command");
    assert_eq!(r.h.run_kind(), RunKind::Dictation);
    r.h.send(Command::Stop);
    wait_for(&r.h, |s| *s == HudState::Hidden).await;
}

#[tokio::test]
async fn in_hold_mode_a_command_started_with_the_modifier_first_ends_on_release() {
    let r = command_rig_in(RecordMode::Hold, Some("teh text"), "the text");
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { mode: RecordMode::Hold })).await;
    r.h.send(Command::KeyToCommand);
    wait_for_run_kind(&r, RunKind::Command).await;
    settle().await;
    assert_eq!(*r.h.hud().borrow(), HudState::Listening { mode: RecordMode::Hold });
    r.h.send(Command::CommandKeyUp);
    wait_for(&r.h, |s| *s == HudState::Replaced).await;
    assert_eq!(*r.replaced.lock().unwrap(), ["the text"]);
    assert_eq!(counts(&r), (1, 2));
}

#[tokio::test]
async fn in_hold_mode_each_shortcut_ends_only_the_recording_it_started() {
    let r = command_rig_in(RecordMode::Hold, Some("teh text"), "the text");
    r.h.send(Command::CommandKeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { mode: RecordMode::Hold })).await;
    r.h.send(Command::KeyDown);
    r.h.send(Command::KeyUp);
    settle().await;
    assert!(matches!(*r.h.hud().borrow(), HudState::Listening { .. }), "the other shortcut is ignored");
    r.h.send(Command::CommandKeyUp);
    wait_for(&r.h, |s| *s == HudState::Replaced).await;

    let r = rig(
        MockTranscriber::new("hi"),
        MockTextModel::default(),
        RigOptions { mode: RecordMode::Hold, ..Default::default() },
    );
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    r.h.send(Command::CommandKeyDown);
    r.h.send(Command::CommandKeyUp);
    settle().await;
    assert!(matches!(*r.h.hud().borrow(), HudState::Listening { .. }));
    r.h.send(Command::Cancel);
}

#[tokio::test]
async fn escape_cancels_a_listening_command_in_both_modes_and_leaves_nothing_behind() {
    for mode in [RecordMode::Toggle, RecordMode::Hold] {
        let r = command_rig_in(mode, Some("teh text"), "the text");
        r.h.send(Command::CommandKeyDown);
        wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
        r.h.send(Command::Cancel);
        wait_for(&r.h, |s| *s == HudState::Hidden).await;
        settle().await;
        assert_eq!(r.asr.calls(), 0);
        assert!(r.replaced.lock().unwrap().is_empty());
        assert!(r.store.list_history(&HistoryFilter::default()).unwrap().is_empty());
    }
}

#[tokio::test]
async fn a_command_handed_over_to_the_dictation_shortcut_finishes_as_a_dictation() {
    let r = command_rig_in(RecordMode::Toggle, Some("teh text"), "the text");
    r.h.send(Command::CommandKeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
    assert_eq!(r.h.run_kind(), RunKind::Command);
    r.h.send(Command::CommandToKey);
    wait_for_run_kind(&r, RunKind::Dictation).await;
    r.h.send(Command::KeyUp);
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| *s == HudState::Inserted).await;
    assert_eq!(r.sink.delivered.lock().unwrap().len(), 1);
    assert!(r.replaced.lock().unwrap().is_empty());
}

#[tokio::test]
async fn handover_and_chord_messages_without_a_recording_do_nothing() {
    let r = command_rig_in(RecordMode::Toggle, Some("teh text"), "the text");
    press(&r, &[Command::KeyToCommand, Command::CommandToKey, Command::ChordAbort]);
    settle().await;
    assert_eq!(*r.h.hud().borrow(), HudState::Hidden);
    assert_eq!(counts(&r), (0, 0));
}

mod chord_hold;
