use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_trait::async_trait;

use super::*;
use crate::error::{ErrorKind, ProviderError, ProviderResult};
use crate::provider::{ChatDelta, ChatRequest, TextRequest};
use crate::store::{Sources, Speaker};
use crate::transcription::SessionState;

/// Answers a fold request with four points and a refresh request with a topic and one line, and keeps every request.
#[derive(Default)]
struct Scripted {
    requests: Mutex<Vec<ChatRequest>>,
    fail: AtomicBool,
}

impl Scripted {
    fn count(&self, needle: &str) -> usize {
        self.requests.lock().unwrap().iter().filter(|r| r.messages[0].content.contains(needle)).count()
    }

    fn folds(&self) -> usize {
        self.count("Merge the earlier points")
    }

    fn refreshes(&self) -> usize {
        self.count("Write the notes for the transcript")
    }
}

#[async_trait]
impl TextModel for Scripted {
    async fn polish(&self, _: &TextRequest<'_>) -> ProviderResult<String> {
        unreachable!("the notes use chat")
    }

    async fn chat(&self, req: &ChatRequest, _: ChatDelta<'_>, _: &ChatCancel) -> ProviderResult<String> {
        self.requests.lock().unwrap().push(req.clone());
        if self.fail.load(Ordering::SeqCst) {
            return Err(ProviderError::new(ErrorKind::Unavailable, "down"));
        }
        Ok(if req.messages[0].content.contains("Merge the earlier points") {
            "決定一\n決定二\n宿題三\n未決四".into()
        } else {
            "Topic: 金曜の再テスト\n[00:01] 修正を反映する".into()
        })
    }
}

/// Hands out the scripted model as the app would a local or a cloud chat model.
struct Source {
    model: Arc<Scripted>,
    local: bool,
}

impl NotesModelSource for Source {
    fn model(&self) -> ModelFuture<'_> {
        Box::pin(async {
            let (limits, thinking_off) = if self.local { (Limits::local(None), true) } else { (Limits::CLOUD, false) };
            Ok(NotesModel { model: self.model.clone(), limits, thinking_off, hold: None })
        })
    }
}

struct Rig {
    store: Arc<Store>,
    id: i64,
    status: watch::Sender<TranscriptionStatus>,
    model: Arc<Scripted>,
    handle: NotesHandle,
    said: usize,
}

const REFRESH: Duration = Duration::from_millis(500);

fn recording(id: i64, paused: bool, closed: usize) -> TranscriptionStatus {
    TranscriptionStatus {
        state: SessionState::Recording {
            id,
            started_ts: 0,
            sources: Sources::default(),
            recorded_ms: 0,
            segment_start: (!paused).then_some(1),
        },
        closed,
        ..Default::default()
    }
}

impl Rig {
    fn start(local: bool) -> Self {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let id = store.create_transcript(0, Sources::default()).unwrap();
        let (status, rx) = watch::channel(recording(id, false, 0));
        let model = Arc::new(Scripted::default());
        let handle = spawn(
            NotesDeps {
                store: store.clone(),
                status: rx,
                source: Arc::new(Source { model: model.clone(), local }),
                me: "自分".into(),
                others: "相手".into(),
                offset_secs: 0,
                cadence: Cadence { look_every: Duration::from_millis(15), refresh_every: REFRESH },
            },
            id,
            None,
        );
        Self { store, id, status, model, handle, said: 0 }
    }

    fn say(&mut self, start_ms: u64, text: &str) {
        let p = self.store.add_paragraph(self.id, Speaker::Others, start_ms, start_ms + 2_000, text).unwrap();
        self.store.finish_paragraph(p, text, None).unwrap();
        self.said += 1;
        self.status.send_modify(|s| s.closed = self.said);
    }

    fn set_paused(&self, paused: bool) {
        let closed = self.said;
        self.status.send_replace(recording(self.id, paused, closed));
    }

    fn end(&self) {
        self.status.send_modify(|s| s.state = SessionState::Idle);
    }

    async fn wait(&self, ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }

    async fn until(&self, what: &str, mut ok: impl FnMut(&Rig) -> bool) {
        for _ in 0..200 {
            if ok(self) {
                return;
            }
            self.wait(10).await;
        }
        panic!("timed out waiting for {what}");
    }
}

#[tokio::test]
async fn the_first_open_writes_notes_at_once_from_the_text_there_is() {
    let mut rig = Rig::start(true);
    assert_eq!(rig.handle.view().phase, NotesPhase::Waiting);
    rig.wait(60).await;
    assert_eq!(rig.model.refreshes(), 0, "nothing to write about yet");
    rig.say(1_000, "金曜に再テストします。");
    rig.until("the first notes", |r| r.handle.view().phase == NotesPhase::Ready && r.handle.view().notes.is_some())
        .await;
    let view = rig.handle.view();
    let notes = view.notes.unwrap();
    assert_eq!((notes.topic.as_str(), notes.recent.len(), notes.covered_ms), ("金曜の再テスト", 1, 3_000));
    assert_eq!(rig.model.refreshes(), 1);
    assert_eq!(rig.model.folds(), 0, "a short meeting has nothing to fold");
}

#[tokio::test]
async fn new_text_refreshes_once_per_interval_and_no_text_never_does() {
    let mut rig = Rig::start(true);
    rig.say(0, "最初の発言です。");
    rig.until("the first refresh", |r| r.model.refreshes() == 1).await;
    rig.wait(2 * REFRESH.as_millis() as u64).await;
    assert_eq!(rig.model.refreshes(), 1, "the same text is never summarized twice");

    rig.say(10_000, "二つ目の発言です。");
    rig.until("the second refresh", |r| r.model.refreshes() == 2).await;
    rig.say(20_000, "三つ目の発言です。");
    rig.wait(100).await;
    assert_eq!(rig.model.refreshes(), 2, "new text waits for the interval");
    rig.until("the third refresh", |r| r.model.refreshes() == 3).await;
    rig.wait(2 * REFRESH.as_millis() as u64).await;
    assert_eq!(rig.model.refreshes(), 3, "the same text is never summarized twice");
}

#[tokio::test]
async fn a_pause_holds_refreshes_and_the_end_writes_the_last_notes_once() {
    let mut rig = Rig::start(true);
    rig.say(0, "最初の発言です。");
    rig.until("the first refresh", |r| r.model.refreshes() == 1).await;
    rig.set_paused(true);
    rig.say(5_000, "止めている間に届いた発言です。");
    rig.wait(3 * REFRESH.as_millis() as u64).await;
    assert_eq!(rig.model.refreshes(), 1, "no refresh while paused");

    rig.end();
    rig.until("the final refresh", |r| r.model.refreshes() == 2).await;
    rig.wait(2 * REFRESH.as_millis() as u64).await;
    assert_eq!(rig.model.refreshes(), 2, "the meeting is covered, so the work ends");
    assert_eq!(rig.handle.view().notes.unwrap().covered_ms, 7_000);
}

#[tokio::test]
async fn a_long_meeting_shows_its_notes_first_and_folds_what_aged_out_afterwards() {
    let mut rig = Rig::start(true);
    let minute = 60_000;
    for i in 0..14 {
        rig.say(i * minute, &format!("{i}分目の発言です。決定事項があります。"));
    }
    rig.until("the first notes", |r| r.handle.view().notes.is_some()).await;
    {
        let requests = rig.model.requests.lock().unwrap();
        assert!(
            requests[0].messages[0].content.contains("Write the notes for the transcript"),
            "the first request writes the notes, not a fold"
        );
    }
    rig.until("the folded notes", |r| r.handle.view().notes.is_some_and(|n| !n.earlier.is_empty())).await;
    let notes = rig.handle.view().notes.unwrap();
    assert_eq!(notes.earlier, ["決定一", "決定二", "宿題三", "未決四"]);
    let (from, until) = notes.earlier_span_ms.unwrap();
    assert_eq!(from, 0);
    assert!(until <= notes.covered_ms - RECENT_WINDOW_MS, "only what left the window is folded");
    assert_eq!(rig.model.folds(), 1);
    assert_eq!(rig.model.refreshes(), 1);

    rig.say(14 * minute, "十四分目の発言です。");
    rig.until("the next refresh", |r| r.model.refreshes() == 2).await;
    assert_eq!(rig.model.folds(), 1, "less than a window's worth aged out, so nothing is folded again");
    let requests = rig.model.requests.lock().unwrap();
    assert!(requests.last().unwrap().messages[1].content.contains("<point>決定一</point>"));
}

#[tokio::test]
async fn a_very_long_meeting_folds_in_short_requests_and_shows_each_step() {
    let mut rig = Rig::start(true);
    let minute = 60_000;
    let long = "あ".repeat(1_500);
    for i in 0..24 {
        rig.say(i * minute, &format!("{i}{long}"));
    }
    rig.until("the first notes", |r| r.handle.view().notes.is_some()).await;
    rig.until("the folds", |r| r.model.folds() >= 4).await;
    rig.wait(100).await;
    let requests = rig.model.requests.lock().unwrap();
    let fold_sizes: Vec<usize> = requests
        .iter()
        .filter(|r| r.messages[0].content.contains("Merge the earlier points"))
        .map(|r| r.messages[1].content.chars().count())
        .collect();
    assert!(fold_sizes.len() >= 4, "{fold_sizes:?}");
    assert!(fold_sizes.iter().all(|n| *n < 6_500), "each request stays short: {fold_sizes:?}");
    assert!(requests[0].messages[0].content.contains("Write the notes"));
    let refresh_len = requests[0].messages[1].content.chars().count();
    assert!(refresh_len < 14_000, "the first request does not read the whole 36 000 characters: {refresh_len}");
}

#[tokio::test]
async fn a_cloud_chat_model_writes_the_notes_with_its_own_reasoning() {
    let mut rig = Rig::start(false);
    rig.say(0, "最初の発言です。");
    rig.until("the first notes", |r| r.handle.view().phase == NotesPhase::Ready && r.handle.view().notes.is_some())
        .await;
    assert_eq!(rig.handle.view().notes.unwrap().topic, "金曜の再テスト");
    let requests = rig.model.requests.lock().unwrap();
    assert!(!requests.is_empty());
    assert!(requests.iter().all(|r| r.thinking.is_none()), "a cloud model keeps its configured reasoning");
}

#[tokio::test]
async fn a_failed_refresh_keeps_the_last_notes_and_says_they_are_behind() {
    let mut rig = Rig::start(true);
    rig.say(0, "最初の発言です。");
    rig.until("the first notes", |r| r.handle.view().notes.is_some()).await;
    let first = rig.handle.view().notes.unwrap();
    rig.model.fail.store(true, Ordering::SeqCst);
    rig.say(10_000, "二つ目の発言です。");
    rig.until("the failure", |r| r.handle.view().stale).await;
    let view = rig.handle.view();
    assert_eq!((view.phase, view.notes.as_ref()), (NotesPhase::Ready, Some(&first)));
    rig.model.fail.store(false, Ordering::SeqCst);
    rig.until("recovery", |r| !r.handle.view().stale && r.handle.view().notes.is_some_and(|n| n.covered_ms == 12_000))
        .await;
}

#[tokio::test]
async fn dropping_the_handle_ends_the_work_even_mid_request() {
    struct Slow(Arc<AtomicUsize>);
    #[async_trait]
    impl TextModel for Slow {
        async fn polish(&self, _: &TextRequest<'_>) -> ProviderResult<String> {
            unreachable!()
        }
        async fn chat(&self, _: &ChatRequest, _: ChatDelta<'_>, _: &ChatCancel) -> ProviderResult<String> {
            struct Counted(Arc<AtomicUsize>);
            impl Drop for Counted {
                fn drop(&mut self) {
                    self.0.fetch_add(1, Ordering::SeqCst);
                }
            }
            let _alive = Counted(self.0.clone());
            std::future::pending().await
        }
    }
    let dropped = Arc::new(AtomicUsize::new(0));
    let mut rig = Rig::start(true);
    struct SlowSource(Arc<AtomicUsize>);
    impl NotesModelSource for SlowSource {
        fn model(&self) -> ModelFuture<'_> {
            Box::pin(async {
                Ok(NotesModel {
                    model: Arc::new(Slow(self.0.clone())),
                    limits: Limits::local(None),
                    thinking_off: true,
                    hold: None,
                })
            })
        }
    }
    drop(std::mem::replace(
        &mut rig.handle,
        spawn(
            NotesDeps {
                store: rig.store.clone(),
                status: rig.status.subscribe(),
                source: Arc::new(SlowSource(dropped.clone())),
                me: String::new(),
                others: String::new(),
                offset_secs: 0,
                cadence: Cadence { look_every: Duration::from_millis(15), refresh_every: REFRESH },
            },
            rig.id,
            None,
        ),
    ));
    rig.say(0, "最初の発言です。");
    rig.until("the request to start", |r| matches!(r.handle.view().phase, NotesPhase::Working { .. })).await;
    rig.wait(50).await;
    assert_eq!(dropped.load(Ordering::SeqCst), 0, "the request is still waiting");
    drop(rig.handle);
    for _ in 0..100 {
        if dropped.load(Ordering::SeqCst) == 1 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the request outlived the handle");
}
