use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use async_trait::async_trait;

use super::*;
use crate::error::{ErrorKind, ProviderError, ProviderResult};
use crate::meeting_notes::Item;
use crate::provider::{ChatDelta, ChatRequest, TextRequest, Thinking};
use crate::store::{Sources, Speaker};
use crate::transcription::SessionState;

/// Answers a hint with a topic and a memo with one line per group, in the language the request names,
/// and keeps every request.
#[derive(Default)]
struct Scripted {
    requests: Mutex<Vec<ChatRequest>>,
    fail: AtomicBool,
    topic: Mutex<Option<String>>,
    memo_replies: Mutex<VecDeque<&'static str>>,
    memo_delay_ms: AtomicU32,
    in_flight: AtomicUsize,
    /// Tokens the model reasons for unless thinking is off, out of the same `max_tokens` as the reply, as Bedrock's
    /// adaptive thinking does. The configured limit stands in for an unset one.
    thinking_tokens: AtomicU32,
}

const CONFIGURED_MAX_TOKENS: u32 = 2048;

const HINT: &str = "You name the current topic";
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
        struct Request<'a>(&'a AtomicUsize);
        impl Drop for Request<'_> {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }
        assert_eq!(self.in_flight.fetch_add(1, Ordering::SeqCst), 0, "notes requests must be serial");
        let _request = Request(&self.in_flight);
        self.requests.lock().unwrap().push(req.clone());
        let memo = req.messages[0].content.contains(MEMO);
        if memo {
            let delay = self.memo_delay_ms.load(Ordering::SeqCst);
            if delay > 0 {
                tokio::time::sleep(Duration::from_millis(delay.into())).await;
            }
            if let Some(reply) = self.memo_replies.lock().unwrap().pop_front() {
                return Ok(reply.into());
            }
        } else if let Some(topic) = self.topic.lock().unwrap().as_ref() {
            return Ok(format!("NOW: {topic}"));
        }
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
        Ok(match (memo, english) {
            (true, false) => {
                "DECIDED: 金曜に決めた\nTODO: 自分: 宿題を出す\nOPEN: 未決のまま\nFLOW: [00:00] 流れを確認\nEND"
            }
            (true, true) => "DECIDED: decision one\nFLOW: [00:00] flow two\nEND",
            (false, false) => "NOW: 金曜の再テスト",
            (false, true) => "NOW: Friday retest",
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
            Ok(NotesModel {
                model: self.model.clone(),
                limits,
                local: self.local,
                thinking_off,
                max_tokens,
                hold: None,
            })
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

const HINT_INTERVAL: Duration = Duration::from_millis(150);
const CADENCE: Cadence =
    Cadence { look_every: Duration::from_millis(15), hint_every: HINT_INTERVAL, final_gap: Duration::from_millis(25) };

fn recording(id: i64, paused: bool, closed: usize) -> TranscriptionStatus {
    TranscriptionStatus {
        state: SessionState::Recording {
            id,
            started_ts: 0,
            sources: Sources::default(),
            recorded_ms: 0,
            segment_start: (!paused).then(crate::unix_seconds),
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
                cadence: CADENCE,
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

    fn advance_to(&self, ms: u64) {
        self.status.send_modify(|status| {
            if let SessionState::Recording { recorded_ms, .. } = &mut status.state {
                *recorded_ms = ms;
            }
        });
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

    async fn wait_for_actor_end(&self) {
        let mut rx = self.handle.subscribe();
        tokio::time::timeout(Duration::from_secs(2), async { while rx.changed().await.is_ok() {} })
            .await
            .expect("the actor exits after the final batch");
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
    assert_eq!((now.topic.as_str(), notes.covered_ms()), ("金曜の再テスト", 3_000));
    assert_eq!(rig.model.hints(), 1);
    assert_eq!(rig.model.memos(), 0, "a short meeting has nothing for the memo yet");
    let requests = rig.model.requests.lock().unwrap();
    assert_eq!(requests[0].max_tokens, Some(80), "a model that does not think keeps the short budget");
}

#[tokio::test]
async fn new_text_refreshes_once_per_interval_and_no_text_never_does() {
    let mut rig = Rig::start(true);
    rig.say(0, "最初の発言です。");
    rig.until("the first refresh", |r| r.model.hints() == 1).await;
    rig.wait(2 * HINT_INTERVAL.as_millis() as u64).await;
    assert_eq!(rig.model.hints(), 1, "the same text is never summarized twice");

    rig.say(10_000, "二つ目の発言です。");
    rig.until("the second refresh", |r| r.model.hints() == 2).await;
    rig.say(20_000, "三つ目の発言です。");
    rig.wait(100).await;
    assert_eq!(rig.model.hints(), 2, "new text waits for the interval");
    rig.until("the third refresh", |r| r.model.hints() == 3).await;
    rig.wait(2 * HINT_INTERVAL.as_millis() as u64).await;
    assert_eq!(rig.model.hints(), 3, "the same text is never summarized twice");
}

#[tokio::test]
async fn a_pause_holds_refreshes_and_the_end_writes_the_last_notes_once() {
    let mut rig = Rig::start(true);
    rig.say(0, "最初の発言です。");
    rig.until("the first refresh", |r| r.model.hints() == 1).await;
    rig.set_paused(true);
    rig.say(5_000, "止めている間に届いた発言です。");
    rig.wait(3 * HINT_INTERVAL.as_millis() as u64).await;
    assert_eq!(rig.model.hints(), 1, "no refresh while paused");

    rig.end();
    rig.until("the final memo", |r| {
        r.handle.view().ended && r.handle.view().notes.is_some_and(|n| n.memo.covered_ms == 7_000)
    })
    .await;
    rig.wait(2 * HINT_INTERVAL.as_millis() as u64).await;
    assert_eq!((rig.model.hints(), rig.model.memos()), (1, 1));
    assert_eq!(rig.handle.view().notes.unwrap().covered_ms(), 7_000);
}

#[tokio::test]
async fn a_long_meeting_shows_its_hint_first_and_writes_a_memo_through_the_latest_speech() {
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
    assert_eq!(until, 13 * minute + 2_000);
    assert!(notes.now.is_none(), "the memo retires the covered topic");
    assert_eq!((rig.model.memos(), rig.model.hints()), (1, 1));

    rig.say(14 * minute, "十四分目の発言です。");
    rig.until("the next refresh", |r| r.model.hints() == 2).await;
    assert_eq!(rig.model.memos(), 1, "the next five-minute interval has yet to pass");
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
    assert_eq!(view.stale, Stale::default(), "the reasoning left room for the notes");
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
    rig.until("the first failed hint", |r| r.model.hints() == 2 && r.handle.view().phase == NotesPhase::Ready).await;
    assert!(!rig.handle.view().stale.hint);
    rig.until("the second failed hint", |r| r.handle.view().stale.hint).await;
    let view = rig.handle.view();
    assert_eq!((view.phase, view.notes.as_ref()), (NotesPhase::Ready, Some(&first)));
    rig.model.fail.store(false, Ordering::SeqCst);
    rig.until("recovery", |r| {
        !r.handle.view().stale.hint && r.handle.view().notes.is_some_and(|n| n.covered_ms() == 12_000)
    })
    .await;
}

fn has_kana(text: &str) -> bool {
    text.chars().any(|c| ('\u{3041}'..='\u{30ff}').contains(&c))
}

#[tokio::test]
async fn a_japanese_meeting_is_written_in_japanese_by_every_request() {
    let mut rig = Rig::start(false);
    rig.model.memo_delay_ms.store(100, Ordering::SeqCst);
    let minute = 60_000;
    rig.say(0, "金曜の再テストに間に合うよう、今日中に修正パッチを出しましょう。表示崩れの二件は直りました。");
    for i in 1..11 {
        rig.say(i * minute, &format!("{i}分目です。配送設定は初期値をオフにします。QA には明日見てもらいます。"));
    }
    rig.until("the hint", |r| r.handle.view().notes.is_some_and(|n| n.now.is_some())).await;
    assert!(has_kana(&rig.handle.view().notes.unwrap().now.unwrap().topic));
    rig.until("the memo", |r| r.handle.view().notes.is_some_and(|n| !n.memo.is_empty())).await;
    let notes = rig.handle.view().notes.unwrap();
    assert_eq!(notes.lang, Some(Lang::Ja));
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
        async fn chat(&self, _: &ChatRequest, _: ChatDelta<'_>, cancel: &ChatCancel) -> ProviderResult<String> {
            struct Counted<'a>(&'a AtomicUsize, &'a ChatCancel);
            impl Drop for Counted<'_> {
                fn drop(&mut self) {
                    assert!(self.1.is_cancelled(), "the provider is told to stop before its future is dropped");
                    self.0.fetch_add(1, Ordering::SeqCst);
                }
            }
            let _alive = Counted(&self.0, cancel);
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
                    local: true,
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
                cadence: CADENCE,
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

#[tokio::test]
async fn a_hint_runs_between_memo_chunks_without_overlapping_them() {
    let mut rig = Rig::start(true);
    rig.model.memo_delay_ms.store(220, Ordering::SeqCst);
    let paragraph = "あ".repeat(3_100);
    rig.say(0, &paragraph);
    rig.say(120_000, &paragraph);
    rig.until("the first memo chunk in flight", |r| r.model.memos() == 1).await;

    rig.say(130_000, "いまの議題は検証です。");
    rig.until("the next memo chunk in flight", |r| r.model.memos() == 2).await;
    {
        let requests = rig.model.requests.lock().unwrap();
        let order: Vec<_> = requests.iter().map(|r| r.messages[0].content.contains(MEMO)).collect();
        assert_eq!(order, [false, true, false, true]);
    }
    rig.until("the memo covers its original batch", |r| {
        r.handle.view().notes.is_some_and(|n| n.memo.covered_ms == 122_000)
    })
    .await;
    let notes = rig.handle.view().notes.unwrap();
    assert_eq!(notes.now.unwrap().covered_ms, 132_000, "a newer hint survives the memo");
}

#[tokio::test]
async fn the_trail_merges_consecutive_topics_and_retires_at_the_memo_watermark() {
    let mut rig = Rig::start(true);
    for (i, topic) in ["Alpha", "Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta"].into_iter().enumerate() {
        *rig.model.topic.lock().unwrap() = Some(topic.into());
        let at = i as u64 * 10_000;
        rig.say(at, &format!("発言 {i}"));
        rig.until("the next hint", |r| {
            r.handle.view().notes.and_then(|n| n.now).is_some_and(|h| h.covered_ms == at + 2_000)
        })
        .await;
        let notes = rig.handle.view().notes.unwrap();
        assert!(notes.trail.len() <= MAX_TRAIL);
        if i == 1 {
            assert!(notes.trail.is_empty(), "the same topic just advances its coverage");
        }
        if i == 2 {
            assert_eq!(notes.trail, [Hint { topic: "Alpha".into(), covered_ms: 12_000 }]);
        }
    }
    let notes = rig.handle.view().notes.unwrap();
    let topics: Vec<_> = notes.trail.iter().map(|hint| hint.topic.as_str()).collect();
    assert_eq!(topics, ["Gamma", "Delta", "Epsilon"]);

    rig.advance_to(300_000);
    rig.until("the memo", |r| r.handle.view().notes.is_some_and(|n| n.memo.covered_ms == 62_000)).await;
    let notes = rig.handle.view().notes.unwrap();
    assert!(notes.trail.is_empty());
    assert!(notes.now.is_none());
    let requests = rig.model.requests.lock().unwrap();
    let memo = requests.last().unwrap();
    assert!(memo.messages[0].content.contains(MEMO));
    assert!(!last_user(memo).contains("Epsilon") && !last_user(memo).contains("Zeta"));
}

#[tokio::test]
async fn a_failed_chunk_keeps_the_last_watermark_and_retries_only_uncovered_speech() {
    let mut rig = Rig::start(true);
    rig.model.memo_replies.lock().unwrap().extend(["DECIDED: first good memo\nEND", "DECIDED: partial"]);
    let paragraph = "あ".repeat(3_100);
    for i in 0..3 {
        rig.say(i * 120_000, &format!("segment-{i} {paragraph}"));
    }
    rig.until("the failed second chunk", |r| r.handle.view().stale.memo).await;
    let view = rig.handle.view();
    assert_eq!((view.stale.hint, view.stale.memo), (false, true));
    let memo = view.notes.unwrap().memo;
    assert_eq!(memo.covered_ms, 2_000);
    assert_eq!(memo.decided[0].text, "first good memo");

    rig.until("the resumed memo", |r| {
        let view = r.handle.view();
        !view.stale.memo && view.notes.is_some_and(|n| n.memo.covered_ms == 242_000)
    })
    .await;
    let requests = rig.model.requests.lock().unwrap();
    let memos: Vec<_> = requests.iter().filter(|r| r.messages[0].content.contains(MEMO)).map(last_user).collect();
    assert_eq!(memos.len(), 4);
    for (request, expected) in memos.iter().zip([0, 1, 1, 2]) {
        assert!(request.contains(&format!("segment-{expected}")));
        assert_eq!((0..3).filter(|i| request.contains(&format!("segment-{i}"))).count(), 1);
    }
    assert!(memos[2].contains("first good memo"));
}

#[tokio::test]
async fn a_bad_final_memo_never_replaces_the_previous_memo_or_retries() {
    for reply in ["", "DECIDED: cut off", "#Purpose#\nEND", "END"] {
        let first = Memo {
            decided: vec![Item { at: None, text: "Keep this decision".into() }],
            span_ms: Some((0, 2_000)),
            covered_ms: 2_000,
            ..Memo::default()
        };
        let seed = Notes { memo: first.clone(), ..Notes::default() };
        let mut rig = Rig::start_with(true, Some(seed));
        rig.model.memo_replies.lock().unwrap().push_back(reply);
        rig.say(0, "最初の発言です。");
        rig.say(60_000, "続きの発言です。");
        rig.end();
        rig.until("the rejected final memo", |r| r.handle.view().stale.memo).await;
        let view = rig.handle.view();
        assert!(view.ended);
        assert_eq!(view.notes.unwrap().memo, first);
        rig.wait_for_actor_end().await;
        assert_eq!((rig.model.hints(), rig.model.memos()), (0, 1));
    }
}

#[tokio::test]
async fn a_finished_long_meeting_uses_one_final_batch_even_when_it_needs_several_chunks() {
    let mut rig = Rig::start(true);
    let paragraph = "あ".repeat(3_100);
    for i in 0..3 {
        rig.say(i * 120_000, &paragraph);
    }
    rig.end();
    rig.wait_for_actor_end().await;
    let view = rig.handle.view();
    assert!(view.ended);
    assert_eq!(view.notes.unwrap().memo.covered_ms, 242_000);
    assert_eq!((rig.model.hints(), rig.model.memos()), (0, 3));
}

#[tokio::test]
async fn ending_is_visible_while_the_in_flight_memo_finishes() {
    let mut rig = Rig::start(true);
    rig.say(0, "最初の発言です。");
    rig.say(120_000, "ふたつ目の発言です。");
    rig.model.memo_delay_ms.store(220, Ordering::SeqCst);
    rig.until("a memo in flight", |r| r.model.memos() == 1).await;
    rig.say(130_000, "最後の発言です。");
    rig.end();

    rig.until("the ended flag", |r| r.handle.view().ended).await;
    assert_eq!(rig.model.in_flight.load(Ordering::SeqCst), 1);
    assert_eq!(rig.model.memos(), 1);
    rig.wait_for_actor_end().await;
    assert_eq!(rig.handle.view().notes.unwrap().memo.covered_ms, 132_000);
    assert_eq!((rig.model.hints(), rig.model.memos()), (1, 2));
}

#[tokio::test]
async fn changes_to_the_live_paragraph_can_refresh_a_hint_at_the_same_timestamp() {
    use crate::transcription::LiveText;
    let rig = Rig::start(true);
    let heard =
        |text: &str| LiveText { speaker: Speaker::Me, start_ms: 0, settled: text.into(), unsettled: String::new() };
    rig.status.send_modify(|s| s.live = vec![heard("確認します。")]);
    rig.until("first hint", |r| r.handle.view().notes.is_some()).await;
    rig.status.send_modify(|s| s.live = vec![heard("確認します。金曜までに連絡します。")]);
    rig.until("a hint for the extended paragraph", |r| r.model.hints() == 2).await;
    let requests = rig.model.requests.lock().unwrap();
    assert!(last_user(&requests[1]).contains("金曜までに連絡します。"));
}

#[tokio::test(start_paused = true)]
async fn each_kind_and_model_location_has_its_own_timeout() {
    struct Hanging;
    #[async_trait]
    impl TextModel for Hanging {
        async fn polish(&self, _: &TextRequest<'_>) -> ProviderResult<String> {
            unreachable!()
        }

        async fn chat(&self, _: &ChatRequest, _: ChatDelta<'_>, _: &ChatCancel) -> ProviderResult<String> {
            std::future::pending().await
        }
    }

    let memo = Job::Memo { last: false };
    for (job, local, seconds) in [(Job::Hint, false, 20), (Job::Hint, true, 45), (memo, false, 60), (memo, true, 180)] {
        let ready = NotesModel {
            model: Arc::new(Hanging),
            limits: Limits::CLOUD,
            local,
            thinking_off: false,
            max_tokens: None,
            hold: None,
        };
        let cancel = ChatCancel::default();
        let began = Instant::now();
        let result = ask(&ready, &ChatRequest::default(), request_timeout(job, ready.local), &cancel).await;
        assert!(matches!(result, Err(Failure::Timeout(_))));
        assert_eq!(began.elapsed(), Duration::from_secs(seconds));
        assert!(cancel.is_cancelled());
    }
}

#[test]
fn a_chunk_never_leaves_an_utterance_behind_its_watermark() {
    let same_end = 10_000;
    let mut context = MeetingContext::empty(1);
    context.utterances = [
        (0, 20_000, "overlapping"),
        (1_000, 2_000, "first"),
        (2_000, same_end, "same-end one"),
        (3_000, same_end, "same-end two"),
    ]
    .map(|(start_ms, end_ms, text)| Utterance { speaker: Speaker::Me, start_ms, end_ms, text: text.into() })
    .into();
    let mut batch = MemoBatch::new(&context, &Memo::default(), false);
    let first = batch.chunk(24);
    assert_eq!(first.iter().map(|u| u.text.as_str()).collect::<Vec<_>>(), ["first"]);
    batch.consumed = first.len();
    let next = batch.chunk(24);
    assert_eq!(next.iter().map(|u| u.text.as_str()).collect::<Vec<_>>(), ["same-end one", "same-end two"]);
    batch.consumed += next.len();
    assert_eq!(batch.chunk(24)[0].text, "overlapping");
}
