use std::sync::Mutex;

use super::*;
use crate::config::{ConfigStore, InsertMethod};
use crate::controller::{Env, FocusWarmer, Frontmost, MusicMute, Recorder, RecorderError, Sink};
use crate::factory::FixedEngines;
use crate::provider::mock::{MockPolisher, MockTranscriber};
use crate::store::HistoryFilter;

struct FakeRecorder {
    fail: bool,
    clip: AudioClip,
}
impl Recorder for FakeRecorder {
    fn start(&self) -> Result<(), RecorderError> {
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

struct RigOptions {
    mic_fails: bool,
    no_insertion_target: bool,
    audio_dir_blocked: bool,
    mode: RecordMode,
    music_mute: MusicMuteMode,
}

impl Default for RigOptions {
    fn default() -> Self {
        Self {
            mic_fails: false,
            no_insertion_target: false,
            audio_dir_blocked: false,
            mode: RecordMode::Toggle,
            music_mute: MusicMuteMode::Off,
        }
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

fn rig(asr: MockTranscriber, polish: MockPolisher, options: RigOptions) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let config = Arc::new(ConfigStore::open(dir.path().join("c.toml")).unwrap());
    config
        .update(|c| {
            c.general.record_mode = options.mode;
            c.general.music_mute = options.music_mute;
        })
        .unwrap();
    let store = Arc::new(Store::open_in_memory().unwrap());
    let sink = Arc::new(FakeSink { no_target: options.no_insertion_target, ..Default::default() });
    let asr = Arc::new(asr);
    let music_mute = FakeMusicMute::default();
    let audio_dir = dir.path().join("audio");
    if options.audio_dir_blocked {
        std::fs::write(&audio_dir, "not a directory").unwrap();
    }
    let h = spawn(Deps {
        env: Env {
            recorder: Arc::new(FakeRecorder { fail: options.mic_fails, clip: AudioClip::new(vec![0.3; 16_000]) }),
            sink: sink.clone(),
            frontmost: Arc::new(FakeFront),
            focus_warmer: Arc::new(NoopFocusWarmer),
            music_mute: Arc::new(music_mute.clone()),
        },
        config,
        store: store.clone(),
        engines: Arc::new(FixedEngines { asr: asr.clone(), polish: Arc::new(polish) }),
        audio_dir: Some(audio_dir),
    });
    Rig { h, sink, store, asr, mute: music_mute, _dir: dir }
}

fn simple_rig(text: &str) -> Rig {
    rig(MockTranscriber::new(text), MockPolisher::default(), RigOptions::default())
}

fn failing_rig(kind: ErrorKind) -> Rig {
    rig(MockTranscriber::new("x").script([Err(kind)]), MockPolisher::default(), RigOptions::default())
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

#[tokio::test(start_paused = true)]
async fn recording_limit_finishes_both_modes_and_ignores_old_timers() {
    for mode in [RecordMode::Toggle, RecordMode::Hold] {
        let r = rig(
            MockTranscriber::new("hello").with_delay(Duration::from_secs(30)),
            MockPolisher::default(),
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
        wait_for(&r.h, |s| *s == HudState::Recognizing).await;
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
async fn toggle_mode_runs_listen_recognize_polish_insert_then_hides() {
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
    assert_eq!((h[0].app.as_str(), h[0].style.clone()), ("Slack", Style::Slack), "style follows the frontmost app");
    assert_eq!(h[0].raw, "えーと 会議です");
    assert!(h[0].audio_path.as_deref().is_some_and(|p| std::path::Path::new(p).exists()));
}

#[tokio::test]
async fn an_unwritable_audio_folder_never_fails_the_dictation() {
    let r = rig(
        MockTranscriber::new("hello"),
        MockPolisher::default(),
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
        MockPolisher::default(),
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
}

#[tokio::test]
async fn stop_during_processing_cancels_and_records_it() {
    let asr = MockTranscriber::new("x").with_delay(Duration::from_secs(30));
    let r = rig(asr, MockPolisher::default(), RigOptions::default());
    r.h.send(Command::KeyDown);
    r.h.send(Command::Finish);
    wait_for(&r.h, |s| *s == HudState::Recognizing).await;
    r.h.send(Command::Stop);
    wait_for(&r.h, |s| *s == HudState::Hidden).await;
    assert!(r.sink.delivered.lock().unwrap().is_empty());
    assert_eq!(history_of(&r.store, 1).await[0].kind, HistoryKind::Cancelled);
}

#[tokio::test]
async fn auth_failure_shows_error_records_history_and_retry_succeeds() {
    let r = rig(
        MockTranscriber::new("やり直し").script([Err(ErrorKind::Auth)]),
        MockPolisher::default(),
        RigOptions::default(),
    );
    r.h.send(Command::KeyDown);
    r.h.send(Command::Finish);
    let s = wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
    assert!(matches!(s, HudState::Error { kind: ErrorKind::Auth, action: ErrorAction::OpenSettings, .. }));
    assert_eq!(history_of(&r.store, 1).await[0].kind, HistoryKind::Failed);

    r.h.send(Command::Retry);
    wait_for(&r.h, |s| *s == HudState::Inserted).await;
    assert_eq!(*r.sink.delivered.lock().unwrap(), ["やり直し。"]);
}

#[tokio::test]
async fn microphone_failure_offers_open_settings() {
    let r =
        rig(MockTranscriber::new("x"), MockPolisher::default(), RigOptions { mic_fails: true, ..Default::default() });
    r.h.send(Command::KeyDown);
    let s = wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
    assert!(matches!(s, HudState::Error { kind: ErrorKind::Mic, action: ErrorAction::OpenSettings, .. }));
}

#[tokio::test]
async fn no_insertion_target_reports_copied() {
    let r = rig(
        MockTranscriber::new("hello"),
        MockPolisher::default(),
        RigOptions { no_insertion_target: true, ..Default::default() },
    );
    r.h.send(Command::KeyDown);
    r.h.send(Command::Finish);
    wait_for(&r.h, |s| *s == HudState::Copied).await;
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
        MockPolisher::default(),
        RigOptions { music_mute: MusicMuteMode::Auto, ..Default::default() },
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
        MockPolisher::default(),
        RigOptions { mic_fails: true, music_mute: MusicMuteMode::Auto, ..Default::default() },
    );
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| matches!(s, HudState::Error { .. })).await;
    assert!(r.mute.log().is_empty(), "no recording means nothing to protect");
}
