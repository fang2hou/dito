use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use async_trait::async_trait;

use super::*;
use crate::error::{ErrorKind, ProviderError, ProviderResult};
use crate::meeting_notes::Item;
use crate::provider::{ChatDelta, ChatRequest, TextRequest, Thinking};
use crate::store::{Sources, Speaker};
use crate::transcription::SessionState;

/// Answers a hint with a topic and one line and a memo with one line per group, in the language the request names,
/// and keeps every request.
#[derive(Default)]
struct Scripted {
    requests: Mutex<Vec<ChatRequest>>,
    fail: AtomicBool,
    /// Tokens the model reasons for unless thinking is off, out of the same `max_tokens` as the reply, as Bedrock's
    /// adaptive thinking does. The configured limit stands in for an unset one.
    thinking_tokens: AtomicU32,
}

const CONFIGURED_MAX_TOKENS: u32 = 2048;

const HINT: &str = "You write a one-glance hint";
const MEMO: &str = "You keep the memo of a meeting";

fn last_user(request: &ChatRequest) -> &str {
    &request.messages.last().unwrap().content
}

impl Scripted {
    fn count(&self, needle: &str) -> usize {
        self.requests.lock().unwrap().iter().filter(|r| r.messages[0].content.contains(needle)).count()
    }

    fn memos(&self) -> usize {
        self.count(MEMO)
    }

    fn hints(&self) -> usize {
        self.count(HINT)
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
        let thinking =
            if req.thinking == Some(Thinking::Off) { 0 } else { self.thinking_tokens.load(Ordering::SeqCst) };
        if thinking >= req.max_tokens.unwrap_or(CONFIGURED_MAX_TOKENS) {
            return Err(ProviderError::new(ErrorKind::BadResponse, "output was cut off at the token limit"));
        }
        // Without a named language the reply follows the sample, which is Japanese in every meeting here.
        let english = last_user(req).starts_with("<meeting_language>English (en)</meeting_language>");
        Ok(match (req.messages[0].content.contains(MEMO), english) {
            (true, false) => {
                "DECIDED: 金曜に決めた\nTODO: 自分: 宿題を出す\nOPEN: 未決のまま\nFLOW: [00:00] 流れを確認\nEND"
            }
            (true, true) => "DECIDED: decision one\nFLOW: [00:00] flow two\nEND",
            (false, false) => "NOW: 金曜の再テスト\nSAID: [00:01] 修正を反映する",
            (false, true) => "NOW: Friday retest\nSAID: [00:01] ship the fix",
        }
        .into())
    }
}

/// Hands out the scripted model as the app would a local or a cloud chat model.
struct Source {
    model: Arc<Scripted>,
    local: bool,
}

/// What a cloud model's settings name as its output limit in these tests.
const CLOUD_CONFIGURED: u32 = 3_000;

impl NotesModelSource for Source {
    fn model(&self) -> ModelFuture<'_> {
        Box::pin(async {
            let (limits, thinking_off) = if self.local { (Limits::local(None), true) } else { (Limits::CLOUD, false) };
            let max_tokens = (!self.local).then_some(CLOUD_CONFIGURED);
            Ok(NotesModel { model: self.model.clone(), limits, thinking_off, max_tokens, hold: None })
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
        Self::start_with(local, None)
    }

    fn start_with(local: bool, seed: Option<Notes>) -> Self {
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
                prior: None,
                offset_secs: 0,
                cadence: Cadence { look_every: Duration::from_millis(15), refresh_every: REFRESH },
            },
            id,
            seed,
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
    assert_eq!(rig.model.hints(), 0, "nothing to write about yet");
    rig.say(1_000, "金曜に再テストします。");
    rig.until("the first notes", |r| r.handle.view().phase == NotesPhase::Ready && r.handle.view().notes.is_some())
        .await;
    let view = rig.handle.view();
    let notes = view.notes.unwrap();
    let now = notes.now.as_ref().unwrap();
    assert_eq!((now.topic.as_str(), now.said.is_some(), notes.covered_ms()), ("金曜の再テスト", true, 3_000));
    assert_eq!(rig.model.hints(), 1);
    assert_eq!(rig.model.memos(), 0, "a short meeting has nothing for the memo yet");
    let requests = rig.model.requests.lock().unwrap();
    assert_eq!(requests[0].max_tokens, Some(160), "a model that does not think keeps the short budget");
}

#[tokio::test]
async fn new_text_refreshes_once_per_interval_and_no_text_never_does() {
    let mut rig = Rig::start(true);
    rig.say(0, "最初の発言です。");
    rig.until("the first refresh", |r| r.model.hints() == 1).await;
    rig.wait(2 * REFRESH.as_millis() as u64).await;
    assert_eq!(rig.model.hints(), 1, "the same text is never summarized twice");

    rig.say(10_000, "二つ目の発言です。");
    rig.until("the second refresh", |r| r.model.hints() == 2).await;
    rig.say(20_000, "三つ目の発言です。");
    rig.wait(100).await;
    assert_eq!(rig.model.hints(), 2, "new text waits for the interval");
    rig.until("the third refresh", |r| r.model.hints() == 3).await;
    rig.wait(2 * REFRESH.as_millis() as u64).await;
    assert_eq!(rig.model.hints(), 3, "the same text is never summarized twice");
}

#[tokio::test]
async fn a_pause_holds_refreshes_and_the_end_writes_the_last_notes_once() {
    let mut rig = Rig::start(true);
    rig.say(0, "最初の発言です。");
    rig.until("the first refresh", |r| r.model.hints() == 1).await;
    rig.set_paused(true);
    rig.say(5_000, "止めている間に届いた発言です。");
    rig.wait(3 * REFRESH.as_millis() as u64).await;
    assert_eq!(rig.model.hints(), 1, "no refresh while paused");

    rig.end();
    rig.until("the final refresh", |r| r.model.hints() == 2).await;
    rig.wait(2 * REFRESH.as_millis() as u64).await;
    assert_eq!(rig.model.hints(), 2, "the meeting is covered, so the work ends");
    assert_eq!(rig.handle.view().notes.unwrap().covered_ms(), 7_000);
}

#[tokio::test]
async fn a_long_meeting_shows_its_hint_first_and_writes_the_memo_of_what_aged_out_afterwards() {
    let mut rig = Rig::start(true);
    let minute = 60_000;
    for i in 0..14 {
        rig.say(i * minute, &format!("{i}分目の発言です。決定事項があります。"));
    }
    rig.until("the first notes", |r| r.handle.view().notes.is_some()).await;
    {
        let requests = rig.model.requests.lock().unwrap();
        assert!(requests[0].messages[0].content.contains(HINT), "the first request is the hint, not the memo");
    }
    rig.until("the memo", |r| r.handle.view().notes.is_some_and(|n| !n.memo.is_empty())).await;
    let notes = rig.handle.view().notes.unwrap();
    let texts = |items: &[Item]| items.iter().map(|i| i.text.clone()).collect::<Vec<_>>();
    assert_eq!(
        (texts(&notes.memo.decided), texts(&notes.memo.todo), texts(&notes.memo.open)),
        (vec!["金曜に決めた".to_owned()], vec!["自分: 宿題を出す".to_owned()], vec!["未決のまま".to_owned()])
    );
    assert_eq!(notes.memo.flow[0].at.as_deref(), Some("00:00"));
    let (from, until) = notes.memo.span_ms.unwrap();
    assert_eq!((from, until), (0, notes.memo.covered_ms));
    assert!(until <= notes.covered_ms() - RECENT_WINDOW_MS, "only what left the window goes to the memo");
    assert_eq!((rig.model.memos(), rig.model.hints()), (1, 1));

    rig.say(14 * minute, "十四分目の発言です。");
    rig.until("the next refresh", |r| r.model.hints() == 2).await;
    assert_eq!(rig.model.memos(), 1, "less than five minutes aged out, so the memo waits");
    assert_eq!(rig.handle.view().notes.unwrap().memo, notes.memo, "the memo stays through a hint");
}

#[tokio::test]
async fn a_very_long_meeting_writes_its_memo_in_short_requests_and_shows_each_step() {
    let mut rig = Rig::start(true);
    let minute = 60_000;
    let long = "あ".repeat(1_500);
    for i in 0..24 {
        rig.say(i * minute, &format!("{i}{long}"));
    }
    rig.until("the first notes", |r| r.handle.view().notes.is_some()).await;
    rig.until("the memos", |r| r.model.memos() >= 4).await;
    rig.wait(100).await;
    let requests = rig.model.requests.lock().unwrap();
    let memo_sizes: Vec<usize> = requests
        .iter()
        .filter(|r| r.messages[0].content.contains(MEMO))
        .map(|r| last_user(r).chars().count())
        .collect();
    assert!(memo_sizes.len() >= 4, "{memo_sizes:?}");
    assert!(memo_sizes.iter().all(|n| *n < 6_800), "each request stays short: {memo_sizes:?}");
    assert!(requests[0].messages[0].content.contains(HINT));
    let hint_len = last_user(&requests[0]).chars().count();
    assert!(hint_len < 2_000, "the hint reads the last minute, not the whole 36 000 characters: {hint_len}");
}

#[tokio::test]
async fn a_cloud_chat_model_writes_the_notes_with_its_own_reasoning() {
    let mut rig = Rig::start(false);
    rig.model.thinking_tokens.store(2_500, Ordering::SeqCst);
    let minute = 60_000;
    for i in 0..11 {
        rig.say(i * minute, &format!("{i}分目の発言です。決定事項があります。"));
    }
    rig.until("the memo", |r| r.handle.view().notes.is_some_and(|n| !n.memo.is_empty())).await;
    let view = rig.handle.view();
    assert!(!view.stale, "the reasoning left room for the notes");
    assert_eq!(view.notes.unwrap().now.unwrap().topic, "金曜の再テスト");
    let requests = rig.model.requests.lock().unwrap();
    assert!(requests.iter().all(|r| r.thinking.is_none()), "a cloud model keeps its configured reasoning");
    for r in requests.iter() {
        let floor = if r.messages[0].content.contains(MEMO) { 4_096 } else { CLOUD_CONFIGURED };
        assert_eq!(r.max_tokens, Some(floor), "the configured limit, raised only where the memo needs more");
    }
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
    rig.until("recovery", |r| {
        !r.handle.view().stale && r.handle.view().notes.is_some_and(|n| n.covered_ms() == 12_000)
    })
    .await;
}

fn has_kana(text: &str) -> bool {
    text.chars().any(|c| ('\u{3041}'..='\u{30ff}').contains(&c))
}

#[tokio::test]
async fn a_japanese_meeting_is_written_in_japanese_by_every_request() {
    let mut rig = Rig::start(false);
    let minute = 60_000;
    rig.say(0, "金曜の再テストに間に合うよう、今日中に修正パッチを出しましょう。表示崩れの二件は直りました。");
    for i in 1..11 {
        rig.say(i * minute, &format!("{i}分目です。配送設定は初期値をオフにします。QA には明日見てもらいます。"));
    }
    rig.until("the memo", |r| r.handle.view().notes.is_some_and(|n| !n.memo.is_empty())).await;
    let notes = rig.handle.view().notes.unwrap();
    assert_eq!(notes.lang, Some(Lang::Ja));
    let now = notes.now.unwrap();
    assert!(has_kana(&now.topic) && has_kana(&now.said.unwrap().text));
    let memo = &notes.memo;
    assert!(
        [&memo.decided, &memo.todo, &memo.open, &memo.flow].iter().flat_map(|g| g.iter()).all(|i| has_kana(&i.text))
    );
    let requests = rig.model.requests.lock().unwrap();
    assert!(requests.len() >= 2);
    for r in requests.iter() {
        let user = last_user(r);
        assert!(user.starts_with("<meeting_language>Japanese (ja)</meeting_language>\n"), "{user}");
    }
}

#[tokio::test]
async fn a_reopened_panel_keeps_the_meeting_language_when_little_is_said() {
    let seed = Notes { lang: Some(Lang::Ja), ..Notes::default() };
    let mut rig = Rig::start_with(true, Some(seed));
    rig.say(0, "OK.");
    rig.until("the first notes", |r| r.handle.view().notes.is_some_and(|n| n.now.is_some())).await;
    assert_eq!(rig.handle.view().notes.unwrap().lang, Some(Lang::Ja));
    let requests = rig.model.requests.lock().unwrap();
    assert!(last_user(&requests[0]).starts_with("<meeting_language>Japanese (ja)</meeting_language>"));
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
                    max_tokens: None,
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
                prior: None,
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
