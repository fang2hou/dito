//! The dictation controller: key events in, HUD states out.
//!
//! One actor task owns the whole session so that rapid key presses, cancels and finishing jobs are
//! serialized. Platform pieces (microphone, text delivery, frontmost app) are injected as traits.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::audio::AudioClip;
use crate::config::{ConfigStore, InsertMethod, MusicMuteMode, RecordMode};
use crate::error::ErrorKind;
use crate::factory::Engines;
use crate::pipeline::{self, Job, Output, Phase, PipelineError};
use crate::prompts::Style;
use crate::store::{HistoryKind, NewHistory, Store};

pub trait Recorder: Send + Sync {
    fn start(&self) -> Result<(), String>;
    /// Stop and return everything recorded since `start`.
    fn stop(&self) -> AudioClip;
    /// Smoothed input level 0..1 for the waveform.
    fn level(&self) -> f32;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    Inserted,
    /// There was no place to insert; the text is on the clipboard.
    Copied,
    Failed(String),
}

pub trait Sink: Send + Sync {
    fn deliver(&self, text: &str, method: InsertMethod) -> Delivery;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppInfo {
    pub name: String,
    pub bundle_id: String,
}

pub trait Frontmost: Send + Sync {
    fn frontmost(&self) -> AppInfo;
}

/// Warms the insertion target's accessibility state when recording starts, so lazily built
/// trees (Electron/Chromium) are ready by delivery. Platform-agnostic no-op in tests.
pub trait FocusWarmer: Send + Sync {
    fn warm(&self);
}

/// Silences other audio (typically music) while the microphone records.
pub trait MusicMute: Send + Sync {
    /// Mute whatever is playing, if anything; dropping the session restores the previous state.
    fn mute_if_active(&self) -> Box<dyn MuteSession>;
}

/// Marker implemented by the guard types [`MusicMute`] returns; the concrete `Drop` restores state.
pub trait MuteSession: Send {}

#[derive(Clone)]
pub struct Env {
    pub recorder: Arc<dyn Recorder>,
    pub sink: Arc<dyn Sink>,
    pub frontmost: Arc<dyn Frontmost>,
    pub focus_warmer: Arc<dyn FocusWarmer>,
    pub music_mute: Arc<dyn MusicMute>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorAction {
    Retry,
    OpenSettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HudState {
    Hidden,
    Listening { mode: RecordMode },
    Recognizing,
    Polishing,
    Inserted,
    Copied,
    Error { kind: ErrorKind, detail: String, action: ErrorAction },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// The recording shortcut went down / up.
    KeyDown,
    KeyUp,
    /// Esc or the × button.
    Cancel,
    /// The ✓ button.
    Finish,
    /// "中止" while recognizing or polishing.
    Stop,
    /// Retry button on an error.
    Retry,
    Dismiss,
    /// Keep an error visible while its controls or explanation are being inspected.
    ErrorHover(bool),
}

enum Msg {
    Cmd(Command),
    Phase(u64, Phase),
    Done(u64, Result<Output, PipelineError>),
    Hide(u64, u64),
    RecordingLimit(u64),
}

/// Maximum duration of a single recording.
pub const MAX_RECORDING_DURATION: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
pub struct DictationHandle {
    tx: mpsc::UnboundedSender<Msg>,
    hud: watch::Receiver<HudState>,
    recording_started: watch::Receiver<Option<Instant>>,
}

impl DictationHandle {
    pub fn send(&self, c: Command) {
        let _ = self.tx.send(Msg::Cmd(c));
    }

    pub fn hud(&self) -> watch::Receiver<HudState> {
        self.hud.clone()
    }

    pub fn recording_started(&self) -> Option<Instant> {
        *self.recording_started.borrow()
    }
}

pub struct Deps {
    pub env: Env,
    pub config: Arc<ConfigStore>,
    pub store: Arc<Store>,
    pub engines: Arc<dyn Engines>,
    /// Where recordings are kept for replay and re-recognition. `None` disables saving.
    pub audio_dir: Option<PathBuf>,
}

struct Meta {
    app: AppInfo,
    style: Style,
    ts: i64,
}

enum St {
    Idle,
    Listening {
        meta: Meta,
        started: Instant,
        /// `Some` while recording has muted other audio; dropped when the microphone stops.
        mute: Option<Box<dyn MuteSession>>,
    },
    Processing {
        meta: Meta,
        clip: Arc<AudioClip>,
        cancel: CancellationToken,
        user_stop: bool,
    },
    /// Terminal HUD state being shown (success flash or error). `clip` allows retry.
    Showing {
        clip: Option<(Arc<AudioClip>, Meta)>,
    },
}

struct Actor {
    deps: Deps,
    hud: watch::Sender<HudState>,
    tx: mpsc::UnboundedSender<Msg>,
    st: St,
    /// Bumped on every transition so late jobs and timers are ignored.
    epoch: u64,
    recording_started: watch::Sender<Option<Instant>>,
    hide_generation: u64,
    hide_paused: bool,
}

pub fn spawn(deps: Deps) -> DictationHandle {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (hud_tx, hud_rx) = watch::channel(HudState::Hidden);
    let (started_tx, started_rx) = watch::channel(None);
    let mut actor = Actor {
        deps,
        hud: hud_tx,
        tx: tx.clone(),
        st: St::Idle,
        epoch: 0,
        recording_started: started_tx,
        hide_generation: 0,
        hide_paused: false,
    };
    tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            actor.handle(m);
        }
    });
    DictationHandle { tx, hud: hud_rx, recording_started: started_rx }
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

// 180 ms settle + 80 ms icon lead-in + 320 ms drawing + 240 ms hold + 260 ms fade as one.
/// Matches the HUD's whole-capsule outcome animation (icon draw + brief hold + fade as one).
const SUCCESS_FLASH: Duration = Duration::from_millis(1080);
/// Fill settle + icon lead-in + copied spring/hold + exit.
const COPIED_FLASH: Duration = Duration::from_millis(1910);
const ERROR_LINGER: Duration = Duration::from_secs(3);

impl Actor {
    fn set_hud(&self, s: HudState) {
        self.hud.send_replace(s);
    }

    fn schedule_hide(&mut self, after: Duration) {
        self.hide_generation += 1;
        self.hide_paused = false;
        let (tx, epoch, generation) = (self.tx.clone(), self.epoch, self.hide_generation);
        tokio::spawn(async move {
            tokio::time::sleep(after).await;
            let _ = tx.send(Msg::Hide(epoch, generation));
        });
    }

    fn go_idle(&mut self) {
        self.hide_paused = false;
        self.epoch += 1;
        self.st = St::Idle;
        self.set_hud(HudState::Hidden);
    }

    fn handle(&mut self, m: Msg) {
        match m {
            Msg::Cmd(c) => self.command(c),
            Msg::Phase(g, p) if g == self.epoch => self.set_hud(match p {
                Phase::Recognizing => HudState::Recognizing,
                Phase::Polishing => HudState::Polishing,
            }),
            Msg::Done(g, r) if g == self.epoch => self.done(r),
            Msg::Hide(g, generation)
                if g == self.epoch
                    && generation == self.hide_generation
                    && !self.hide_paused
                    && matches!(self.st, St::Showing { .. }) =>
            {
                self.go_idle()
            }
            Msg::RecordingLimit(g) if g == self.epoch && matches!(self.st, St::Listening { .. }) => {
                self.finish_listening()
            }
            _ => {}
        }
    }

    fn command(&mut self, c: Command) {
        let mode = self.deps.config.get().general.record_mode;
        match (&self.st, c) {
            (St::Idle | St::Showing { .. }, Command::KeyDown) => self.start_listening(mode),
            (St::Listening { .. }, Command::KeyDown) if mode == RecordMode::Toggle => self.finish_listening(),
            (St::Listening { .. }, Command::KeyUp) if mode == RecordMode::Hold => self.finish_listening(),
            (St::Listening { .. }, Command::Finish) => self.finish_listening(),
            (St::Listening { .. }, Command::Cancel) => self.cancel_listening(),
            (St::Processing { .. }, Command::Cancel | Command::Stop) => {
                if let St::Processing { cancel, user_stop, .. } = &mut self.st {
                    *user_stop = true;
                    cancel.cancel();
                }
            }
            (St::Showing { .. }, Command::ErrorHover(over)) if matches!(*self.hud.borrow(), HudState::Error { .. }) => {
                if over && !self.hide_paused {
                    self.hide_paused = true;
                    self.hide_generation += 1;
                } else if !over && self.hide_paused {
                    // Leaving always grants a full reading interval, even after a late hover.
                    self.schedule_hide(ERROR_LINGER);
                }
            }
            (St::Showing { .. }, Command::Retry) => self.retry(),
            (St::Showing { .. }, Command::Dismiss | Command::Cancel) => self.go_idle(),
            _ => {}
        }
    }

    fn start_listening(&mut self, mode: RecordMode) {
        self.epoch += 1;
        match self.deps.env.recorder.start() {
            Ok(()) => {
                self.deps.env.focus_warmer.warm();
                let mute = (self.deps.config.get().general.music_mute == MusicMuteMode::Auto)
                    .then(|| self.deps.env.music_mute.mute_if_active());
                let meta = Meta { app: self.deps.env.frontmost.frontmost(), style: Style::General, ts: now() };
                let style = Style::for_bundle_id(&meta.app.bundle_id);
                let started = Instant::now();
                self.st = St::Listening { meta: Meta { style, ..meta }, started, mute };
                self.recording_started.send_replace(Some(started));
                self.set_hud(HudState::Listening { mode });
                let (tx, epoch) = (self.tx.clone(), self.epoch);
                let deadline = tokio::time::Instant::now() + MAX_RECORDING_DURATION;
                tokio::spawn(async move {
                    tokio::time::sleep_until(deadline).await;
                    let _ = tx.send(Msg::RecordingLimit(epoch));
                });
            }
            Err(e) => {
                self.st = St::Showing { clip: None };
                self.set_hud(HudState::Error { kind: ErrorKind::Mic, detail: e, action: ErrorAction::OpenSettings });
                self.schedule_hide(ERROR_LINGER);
            }
        }
    }

    fn take_listening(&mut self) -> Option<(Meta, Arc<AudioClip>, u64)> {
        let St::Listening { meta, started, mute } = std::mem::replace(&mut self.st, St::Idle) else { return None };
        self.recording_started.send_replace(None);
        let clip = Arc::new(self.deps.env.recorder.stop());
        drop(mute); // music returns only after the microphone has actually stopped
        Some((meta, clip, started.elapsed().as_millis() as u64))
    }

    fn cancel_listening(&mut self) {
        let Some((meta, clip, _)) = self.take_listening() else { return };
        if !clip.is_effectively_silent() {
            self.record_history(&meta, &clip, HistoryKind::Cancelled, None, None);
        }
        self.go_idle();
    }

    fn finish_listening(&mut self) {
        let Some((meta, clip, _)) = self.take_listening() else { return };
        self.process(meta, clip);
    }

    fn retry(&mut self) {
        let St::Showing { clip } = std::mem::replace(&mut self.st, St::Idle) else { return };
        match clip {
            Some((clip, meta)) => self.process(meta, clip),
            None => self.go_idle(),
        }
    }

    fn process(&mut self, meta: Meta, clip: Arc<AudioClip>) {
        self.epoch += 1;
        let cancel = CancellationToken::new();
        let epoch = self.epoch;
        self.set_hud(HudState::Recognizing);
        let (tx, engines, config, store) =
            (self.tx.clone(), self.deps.engines.clone(), self.deps.config.clone(), self.deps.store.clone());
        let (job_clip, style, token) = (clip.clone(), meta.style, cancel.clone());
        tokio::spawn(async move {
            let cfg = config.get();
            let words = store.words(None).unwrap_or_default();
            let phase_tx = tx.clone();
            let on_phase = move |p: Phase| {
                let _ = phase_tx.send(Msg::Phase(epoch, p));
            };
            let r = pipeline::run(
                engines.as_ref(),
                &cfg,
                &Job { clip: &job_clip, style, dictionary: &words },
                &on_phase,
                &token,
            )
            .await;
            let _ = tx.send(Msg::Done(epoch, r));
        });
        self.st = St::Processing { meta, clip, cancel, user_stop: false };
    }

    fn done(&mut self, r: Result<Output, PipelineError>) {
        let St::Processing { meta, clip, user_stop, .. } = std::mem::replace(&mut self.st, St::Idle) else { return };
        self.epoch += 1;
        match r {
            Ok(out) => {
                let method = self.deps.config.get().general.insert_method;
                let delivery = self.deps.env.sink.deliver(out.text(), method);
                let err = out.polish_error.as_ref().map(|(kind, m)| format!("{kind:?}: {m}"));
                self.record_history(&meta, &clip, HistoryKind::Dictation, Some(&out), err);
                match delivery {
                    Delivery::Inserted => self.set_hud(HudState::Inserted),
                    Delivery::Copied => self.set_hud(HudState::Copied),
                    Delivery::Failed(e) => {
                        self.st = St::Showing { clip: None };
                        self.set_hud(HudState::Error { kind: ErrorKind::Other, detail: e, action: ErrorAction::Retry });
                        self.schedule_hide(ERROR_LINGER);
                        return;
                    }
                }
                self.st = St::Showing { clip: None };
                self.schedule_hide(if matches!(delivery, Delivery::Copied) { COPIED_FLASH } else { SUCCESS_FLASH });
            }
            Err(PipelineError::Cancelled) => {
                debug_assert!(user_stop);
                self.record_history(&meta, &clip, HistoryKind::Cancelled, None, None);
                self.go_idle();
            }
            Err(PipelineError::Failed(e)) => {
                if e.kind != ErrorKind::NoSpeech {
                    self.record_history(&meta, &clip, HistoryKind::Failed, None, Some(e.to_string()));
                }
                self.st = St::Showing { clip: Some((clip, meta)) };
                let action = if matches!(e.kind, ErrorKind::Mic | ErrorKind::Auth) {
                    ErrorAction::OpenSettings
                } else {
                    ErrorAction::Retry
                };
                self.set_hud(HudState::Error { kind: e.kind, detail: e.message, action });
                self.schedule_hide(ERROR_LINGER);
            }
        }
    }

    fn record_history(
        &self,
        meta: &Meta,
        clip: &AudioClip,
        kind: HistoryKind,
        out: Option<&Output>,
        error: Option<String>,
    ) {
        let audio_path = self.deps.audio_dir.as_ref().and_then(|dir| {
            std::fs::create_dir_all(dir).ok()?;
            let path = dir.join(format!("{}-{}.wav", meta.ts, self.epoch));
            std::fs::write(&path, clip.to_wav()).ok()?;
            Some(path.to_string_lossy().into_owned())
        });
        let entry = NewHistory {
            ts: meta.ts,
            kind,
            style: meta.style,
            app: meta.app.name.clone(),
            lang: out.map(|o| o.lang.code().to_string()).unwrap_or_default(),
            duration_ms: clip.duration_ms(),
            raw: out.map(|o| o.raw.clone()).unwrap_or_default(),
            polished: out.and_then(|o| o.polished.clone()),
            error,
            audio_path,
        };
        if let Err(e) = self.deps.store.add_history(&entry) {
            tracing::warn!("history write failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::factory::FixedEngines;
    use crate::provider::mock::{MockPolisher, MockTranscriber};
    use crate::store::HistoryFilter;

    struct FakeRecorder {
        fail: bool,
        clip: AudioClip,
    }
    impl Recorder for FakeRecorder {
        fn start(&self) -> Result<(), String> {
            if self.fail { Err("no permission".into()) } else { Ok(()) }
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
        no_target: bool,
    }
    impl Sink for FakeSink {
        fn deliver(&self, text: &str, _: InsertMethod) -> Delivery {
            self.delivered.lock().unwrap().push(text.into());
            if self.no_target { Delivery::Copied } else { Delivery::Inserted }
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

    /// Records "mute" when asked and "unmute" when the guard drops.
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

    struct Rig {
        h: DictationHandle,
        sink: Arc<FakeSink>,
        store: Arc<Store>,
        asr: Arc<MockTranscriber>,
        mute: FakeMusicMute,
        _dir: tempfile::TempDir,
    }

    fn rig(
        asr: MockTranscriber,
        polish: MockPolisher,
        rec_fail: bool,
        no_target: bool,
        mode: RecordMode,
        mute: MusicMuteMode,
    ) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let config = Arc::new(ConfigStore::open(dir.path().join("c.toml")).unwrap());
        config
            .update(|c| {
                c.general.record_mode = mode;
                c.general.music_mute = mute;
            })
            .unwrap();
        let store = Arc::new(Store::open_in_memory().unwrap());
        let sink = Arc::new(FakeSink { no_target, ..Default::default() });
        let asr = Arc::new(asr);
        let music_mute = FakeMusicMute::default();
        let h = spawn(Deps {
            env: Env {
                recorder: Arc::new(FakeRecorder { fail: rec_fail, clip: AudioClip::new(vec![0.3; 16_000]) }),
                sink: sink.clone(),
                frontmost: Arc::new(FakeFront),
                focus_warmer: Arc::new(NoopFocusWarmer),
                music_mute: Arc::new(music_mute.clone()),
            },
            config,
            store: store.clone(),
            engines: Arc::new(FixedEngines { asr: asr.clone(), polish: Arc::new(polish) }),
            audio_dir: Some(dir.path().join("audio")),
        });
        Rig { h, sink, store, asr, mute: music_mute, _dir: dir }
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

    #[tokio::test(start_paused = true)]
    async fn recording_limit_finishes_both_modes_and_ignores_old_timers() {
        for mode in [RecordMode::Toggle, RecordMode::Hold] {
            let r = rig(
                MockTranscriber::new("hello").with_delay(Duration::from_secs(30)),
                MockPolisher::default(),
                false,
                false,
                mode,
                MusicMuteMode::Off,
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
            wait_for(&r.h, |s| *s == HudState::Recognizing).await;
            assert!(r.h.recording_started().is_none());
        }

        let r = rig(
            MockTranscriber::new("hello"),
            MockPolisher::default(),
            false,
            false,
            RecordMode::Toggle,
            MusicMuteMode::Off,
        );
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

    #[tokio::test]
    async fn toggle_mode_runs_listen_recognize_polish_insert_then_hides() {
        let r = rig(
            MockTranscriber::new("えーと 会議です"),
            MockPolisher::default(),
            false,
            false,
            RecordMode::Toggle,
            MusicMuteMode::Off,
        );
        r.h.send(Command::KeyDown);
        wait_for(&r.h, |s| matches!(s, HudState::Listening { mode: RecordMode::Toggle })).await;
        r.h.send(Command::KeyUp); // releasing the key must not end a toggle recording
        r.h.send(Command::KeyDown);
        wait_for(&r.h, |s| *s == HudState::Inserted).await;
        assert_eq!(*r.sink.delivered.lock().unwrap(), ["会議です。"]);
        wait_for(&r.h, |s| *s == HudState::Hidden).await;

        let h = r.store.list_history(&HistoryFilter::default()).unwrap();
        assert_eq!(h.len(), 1);
        assert_eq!((h[0].app.as_str(), h[0].style), ("Slack", Style::Slack), "style follows the frontmost app");
        assert_eq!(h[0].raw, "えーと 会議です");
        assert!(h[0].audio_path.as_deref().is_some_and(|p| std::path::Path::new(p).exists()));
    }

    #[tokio::test]
    async fn hold_mode_ends_on_key_up_and_ignores_a_second_key_down() {
        let r = rig(
            MockTranscriber::new("hi"),
            MockPolisher::default(),
            false,
            false,
            RecordMode::Hold,
            MusicMuteMode::Off,
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
        let r = rig(
            MockTranscriber::new("x"),
            MockPolisher::default(),
            false,
            false,
            RecordMode::Toggle,
            MusicMuteMode::Off,
        );
        r.h.send(Command::KeyDown);
        wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
        r.h.send(Command::Cancel);
        wait_for(&r.h, |s| *s == HudState::Hidden).await;
        assert!(r.sink.delivered.lock().unwrap().is_empty());
        assert_eq!(r.asr.calls(), 0, "cancel must not recognize");
        let h = r.store.list_history(&HistoryFilter::default()).unwrap();
        assert_eq!(h[0].kind, HistoryKind::Cancelled);
        assert!(h[0].audio_path.is_some(), "the audio is what 再認識 needs");
    }

    #[tokio::test]
    async fn stop_during_processing_cancels_and_records_it() {
        let asr = MockTranscriber::new("x").with_delay(Duration::from_secs(30));
        let r = rig(asr, MockPolisher::default(), false, false, RecordMode::Toggle, MusicMuteMode::Off);
        r.h.send(Command::KeyDown);
        r.h.send(Command::Finish);
        wait_for(&r.h, |s| *s == HudState::Recognizing).await;
        r.h.send(Command::Stop);
        wait_for(&r.h, |s| *s == HudState::Hidden).await;
        assert!(r.sink.delivered.lock().unwrap().is_empty());
        assert_eq!(r.store.list_history(&HistoryFilter::default()).unwrap()[0].kind, HistoryKind::Cancelled);
    }

    #[tokio::test]
    async fn auth_failure_shows_error_records_history_and_retry_succeeds() {
        let r = rig(
            MockTranscriber::new("やり直し").script([Err(ErrorKind::Auth)]),
            MockPolisher::default(),
            false,
            false,
            RecordMode::Toggle,
            MusicMuteMode::Off,
        );
        r.h.send(Command::KeyDown);
        r.h.send(Command::Finish);
        let s = wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
        assert!(matches!(s, HudState::Error { kind: ErrorKind::Auth, action: ErrorAction::OpenSettings, .. }));
        assert_eq!(r.store.list_history(&HistoryFilter::default()).unwrap()[0].kind, HistoryKind::Failed);

        r.h.send(Command::Retry);
        wait_for(&r.h, |s| *s == HudState::Inserted).await;
        assert_eq!(*r.sink.delivered.lock().unwrap(), ["やり直し。"]);
    }

    #[tokio::test]
    async fn microphone_failure_offers_open_settings() {
        let r = rig(
            MockTranscriber::new("x"),
            MockPolisher::default(),
            true,
            false,
            RecordMode::Toggle,
            MusicMuteMode::Off,
        );
        r.h.send(Command::KeyDown);
        let s = wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
        assert!(matches!(s, HudState::Error { kind: ErrorKind::Mic, action: ErrorAction::OpenSettings, .. }));
    }

    #[tokio::test]
    async fn no_insertion_target_reports_copied() {
        let r = rig(
            MockTranscriber::new("hello"),
            MockPolisher::default(),
            false,
            true,
            RecordMode::Toggle,
            MusicMuteMode::Off,
        );
        r.h.send(Command::KeyDown);
        r.h.send(Command::Finish);
        wait_for(&r.h, |s| *s == HudState::Copied).await;
    }

    #[tokio::test(start_paused = true)]
    async fn copied_confirmation_lasts_through_the_longer_label_animation() {
        let r = rig(
            MockTranscriber::new("hello"),
            MockPolisher::default(),
            false,
            true,
            RecordMode::Toggle,
            MusicMuteMode::Off,
        );
        r.h.send(Command::KeyDown);
        r.h.send(Command::Finish);
        wait_for(&r.h, |s| *s == HudState::Copied).await;
        tokio::time::advance(SUCCESS_FLASH).await;
        tokio::task::yield_now().await;
        assert_eq!(*r.h.hud().borrow(), HudState::Copied);
        tokio::time::advance(COPIED_FLASH - SUCCESS_FLASH + Duration::from_millis(10)).await;
        wait_for(&r.h, |s| *s == HudState::Hidden).await;
    }

    #[tokio::test]
    async fn no_speech_is_shown_but_not_recorded_as_a_failure() {
        let r = rig(
            MockTranscriber::new("x").script([Err(ErrorKind::NoSpeech)]),
            MockPolisher::default(),
            false,
            false,
            RecordMode::Toggle,
            MusicMuteMode::Off,
        );
        r.h.send(Command::KeyDown);
        r.h.send(Command::Finish);
        let s = wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
        assert!(matches!(s, HudState::Error { kind: ErrorKind::NoSpeech, .. }));
        assert!(r.store.list_history(&HistoryFilter::default()).unwrap().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn error_is_visible_for_three_seconds_then_closes() {
        let r = rig(
            MockTranscriber::new("x").script([Err(ErrorKind::Network)]),
            MockPolisher::default(),
            false,
            false,
            RecordMode::Toggle,
            MusicMuteMode::Off,
        );
        r.h.send(Command::KeyDown);
        r.h.send(Command::Finish);
        wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
        tokio::time::advance(Duration::from_millis(2990)).await;
        tokio::task::yield_now().await;
        assert!(matches!(*r.h.hud().borrow(), HudState::Error { .. }));
        tokio::time::advance(Duration::from_millis(20)).await;
        wait_for(&r.h, |s| *s == HudState::Hidden).await;
    }

    #[tokio::test(start_paused = true)]
    async fn an_old_error_timeout_cannot_hide_a_new_recording() {
        let r = rig(
            MockTranscriber::new("x").script([Err(ErrorKind::Network)]),
            MockPolisher::default(),
            false,
            false,
            RecordMode::Toggle,
            MusicMuteMode::Off,
        );
        r.h.send(Command::KeyDown);
        r.h.send(Command::Finish);
        wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
        tokio::time::advance(Duration::from_secs(2)).await;
        r.h.send(Command::KeyDown);
        wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
        tokio::time::advance(Duration::from_secs(2)).await;
        tokio::task::yield_now().await;
        assert!(matches!(*r.h.hud().borrow(), HudState::Listening { .. }));
        r.h.send(Command::Cancel);
    }

    #[tokio::test(start_paused = true)]
    async fn hovering_an_error_cancels_and_restarts_its_full_lifetime() {
        let r = rig(
            MockTranscriber::new("x").script([Err(ErrorKind::Network)]),
            MockPolisher::default(),
            false,
            false,
            RecordMode::Toggle,
            MusicMuteMode::Off,
        );
        r.h.send(Command::KeyDown);
        r.h.send(Command::Finish);
        wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
        tokio::time::advance(Duration::from_secs(1)).await;
        r.h.send(Command::ErrorHover(true));
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(10)).await;
        tokio::task::yield_now().await;
        assert!(matches!(*r.h.hud().borrow(), HudState::Error { .. }));
        r.h.send(Command::ErrorHover(false));
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(2900)).await;
        tokio::task::yield_now().await;
        assert!(matches!(*r.h.hud().borrow(), HudState::Error { .. }));
        tokio::time::advance(Duration::from_millis(200)).await;
        wait_for(&r.h, |s| *s == HudState::Hidden).await;
    }

    #[tokio::test]
    async fn auto_mode_mutes_while_listening_and_restores_when_recording_ends() {
        let r = rig(
            MockTranscriber::new("x"),
            MockPolisher::default(),
            false,
            false,
            RecordMode::Toggle,
            MusicMuteMode::Auto,
        );
        r.h.send(Command::KeyDown);
        wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
        assert_eq!(r.mute.log(), ["mute"], "music is muted for the whole listening phase");
        r.h.send(Command::Finish);
        // Recognition and insertion may still be running; the microphone is done, so music returns.
        wait_for(&r.h, |s| *s == HudState::Inserted).await;
        assert_eq!(r.mute.log(), ["mute", "unmute"]);
    }

    #[tokio::test]
    async fn auto_mode_restores_music_when_listening_is_cancelled() {
        let r = rig(
            MockTranscriber::new("x"),
            MockPolisher::default(),
            false,
            false,
            RecordMode::Toggle,
            MusicMuteMode::Auto,
        );
        r.h.send(Command::KeyDown);
        wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await;
        r.h.send(Command::Cancel);
        wait_for(&r.h, |s| *s == HudState::Hidden).await;
        assert_eq!(r.mute.log(), ["mute", "unmute"], "cancel must not leave the music muted");
    }

    #[tokio::test]
    async fn off_mode_never_mutes() {
        let r = rig(
            MockTranscriber::new("x"),
            MockPolisher::default(),
            false,
            false,
            RecordMode::Toggle,
            MusicMuteMode::Off,
        );
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
            MockPolisher::default(),
            true,
            false,
            RecordMode::Toggle,
            MusicMuteMode::Auto,
        );
        r.h.send(Command::KeyDown);
        wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
        assert!(r.mute.log().is_empty(), "no recording means nothing to protect");
    }
}
