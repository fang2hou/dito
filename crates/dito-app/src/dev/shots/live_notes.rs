//! The live meeting notes through the real session actor, page and notes runner (`design:tools/recording/live-notes`):
//! a scripted meeting is recorded while the panel opens and closes, a question is asked and handed to Chat, and the
//! way back from Chat is taken. The chat model is the rule-based mock (`DITO_MOCK_LOCAL_CHAT=1`), so no model is
//! needed. Opt-in: `DITO_SHOTS_ONLY=livenotes`.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::config::Appearance;
use dito_core::factory::FixedEngines;
use dito_core::meeting_notes::{Hint, Item, Memo, Notes, NotesPhase, NotesView, Stale, Unavailable};
use dito_core::provider::mock::{MockTextModel, ToneTranscriber, tone};
use dito_core::transcription::{
    self, ActiveRecording, EnergyDetectors, RecordingPhase, SessionState, Sources, TranscriptionDeps,
    TranscriptionHandle,
};
use gpui_kit::*;

use super::capture::{close_window, render_tools_window, resize_tools_window, save, theme_tag};
use super::input::{click, hover, on_tools_window, redraw};
use super::transcript_film::Talk;
use crate::services;
use crate::state::AppState;
use crate::tools;

async fn settle(cx: &mut AsyncApp, ms: u64) {
    cx.background_executor().timer(Duration::from_millis(ms)).await;
}

async fn shot(cx: &mut AsyncApp, dir: &Path, name: &str, wait: u64) {
    settle(cx, wait).await;
    if let Some(Ok(img)) = render_tools_window(cx) {
        save(dir, name, img);
    }
}

/// Hands the page the session's status at once, as its subscription would between frames.
fn sync(cx: &mut AsyncApp, handle: &TranscriptionHandle) {
    let status = handle.status();
    on_tools_window(cx, |w, app, view| view.update(app, |t, cx| t.sync_transcription(status, w, cx)));
}

fn on_page(
    cx: &mut AsyncApp,
    f: impl FnOnce(&mut Window, &mut App, &Entity<crate::tools::transcription::TranscriptionPage>) + 'static,
) {
    on_tools_window(cx, |w, app, view| {
        let page = view.read(app).transcription().clone();
        f(w, app, &page);
    });
}

fn on_chat(cx: &mut AsyncApp, f: impl FnOnce(&mut Window, &mut App, &Entity<crate::tools::chat::ChatPage>) + 'static) {
    on_tools_window(cx, |w, app, view| {
        let chat = view.read(app).chat().clone();
        f(w, app, &chat);
    });
}

/// A reply with a list, bold text and long Chinese paragraphs, as a chat model writes them.
const MARKDOWN_QUESTION: &str = "这次调查的结论是什么？";
const MARKDOWN_ANSWER: &str = "调查的结论分为三点，**玩家调查**和访谈的结果基本一致：\n\n- **玩家调查**：约70%的受访者认为，即使主角的成长速度放慢，只要剧情的推进足够紧凑，他们也愿意继续玩下去。\n- **访谈结果**：多数玩家更在意战斗的手感，而不是数值的成长，这一点在 `PC` 和主机两个平台上相同。\n- **待确认**：关于付费内容的接受度，样本太少，需要在下一轮调查中补充。\n\n1. 先确定战斗手感的改进方案\n2. 再决定成长曲线是否调整\n\n整体来看，团队倾向于保持现有的节奏，同时把调查中暴露出的问题放进下一个版本的计划里，而不是推迟发布。";

/// Buttons' centres on the 1120 by 720 window, found by looking at the shots.
const BOOK_BUTTON: (f32, f32) = (1045., 67.);
const CONTINUE_IN_CHAT: (f32, f32) = (1032., 131.);
const BACK_TO_NOTES: (f32, f32) = (958., 131.);
const RETURN_CAPSULE: (f32, f32) = (540., 54.);
/// The book button once the window is 900 wide.
const NARROW_BOOK_BUTTON: (f32, f32) = (826., 67.);

fn sample_notes(mixed: bool) -> Notes {
    let hint = |minute: u64, topic: &str| Hint { covered_ms: minute * 60_000, topic: topic.into() };
    let item = |text: &str| Item { at: None, text: text.into() };
    let flow = |at: &str, text: &str| Item { at: Some(at.into()), text: text.into() };
    let topic = if mixed {
        "注文 API を v2 に上げる件と、Safari 17 で崩れる注文フローの再検証に必要な QA の実施範囲と調整日程の確認"
    } else {
        "金曜の再テストに向けた修正"
    };
    let previous = if mixed {
        "Release note の wording と App Store に出す修正内容の確認、QA smoke test の担当と期限"
    } else {
        "配送設定の確認日程"
    };
    let decision = if mixed {
        "注文 API の v2 移行は 12 月の release に含めず、先に webhook の retry を直す。"
    } else {
        "再テストは金曜に実施。"
    };
    Notes {
        now: Some(hint(20, topic)),
        trail: vec![hint(17, "表示崩れの修正状況"), hint(19, previous)],
        memo: Memo {
            decided: vec![item(decision), item("優先するのは表示崩れと配送設定。")],
            todo: vec![item("相手: 今日中に修正パッチを共有。"), item("QA: 明日午前に配送設定を確認。")],
            open: vec![item("ログ出力を今回の修正に含めるか。")],
            flow: vec![
                flow("14:02", "再テストの範囲を確認。"),
                flow("14:09", "表示崩れ2件の原因を共有。"),
                flow("14:14", "配送設定の初期値をオフに決定。"),
            ],
            span_ms: Some((0, 15 * 60_000)),
            covered_ms: 15 * 60_000,
        },
        started_ts: (14 * 60 + 2) * 60 - i64::from(dito_platform::local_utc_offset_secs()),
        ..Notes::default()
    }
}

fn scroll_memo(cx: &mut AsyncApp, delta: f32) {
    on_tools_window(cx, |window, app, _| {
        let position = point(px(920.), px(322.));
        hover(window, app, position);
        let delta = ScrollDelta::Pixels(point(px(0.), px(delta)));
        window.dispatch_event(PlatformInput::Scroll(ScrollEvent { position, delta, ..Default::default() }), app);
        redraw(window, app);
    });
}

async fn panel_states(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp, ended: bool) {
    let notes = sample_notes(false);
    let ready =
        NotesView { phase: NotesPhase::Ready, notes: Some(notes.clone()), stale: Stale::default(), ended, seq: 0 };
    let first = NotesView { notes: None, ..ready.clone() };
    let working = NotesPhase::Working { loading_model: false };
    let mut updated = notes.clone();
    updated.trail.extend(
        updated
            .now
            .replace(Hint { topic: "ログ出力を今回の修正に含めるか".into(), covered_ms: 21 * 60_000 }),
    );
    let states = if ended {
        vec![("ended", ready)]
    } else {
        vec![
            ("waiting", NotesView { phase: NotesPhase::Waiting, ..first.clone() }),
            ("first-notes", NotesView { phase: working.clone(), ..first.clone() }),
            (
                "model-failed",
                NotesView { phase: NotesPhase::Unavailable(Unavailable::ModelFailed("mock startup".into())), ..first },
            ),
            ("ready", ready.clone()),
            ("updating", NotesView { phase: working, ..ready.clone() }),
            ("updated", NotesView { notes: Some(updated), ..ready.clone() }),
            ("topic-stale", NotesView { stale: Stale { hint: true, memo: false }, ..ready.clone() }),
            ("memo-stale", NotesView { stale: Stale { hint: false, memo: true }, ..ready.clone() }),
            ("long-mixed", NotesView { notes: Some(sample_notes(true)), ..ready }),
        ]
    };
    for appearance in [Appearance::Light, Appearance::Dark] {
        services::save_config(st, |c| c.general.appearance = appearance);
        let theme = theme_tag(appearance);
        for (name, view) in &states {
            let height = match *name {
                "topic-stale" | "memo-stale" => 896.,
                "long-mixed" => 908.,
                "ended" => 724.,
                _ => 694.,
            };
            // Coming back from the narrow shots needs extra width to leave the compact layout.
            cx.update(|cx| resize_tools_window(cx, 1160., height));
            let view = view.clone();
            on_page(cx, move |_, app, page| page.update(app, |p, cx| p.pose_notes(Some(view), cx)));
            shot(cx, dir, &format!("livenotes-panel-{name}-{theme}.png"), 400).await;
            if *name == "ready" {
                scroll_memo(cx, -240.);
                shot(cx, dir, &format!("livenotes-panel-memo-scrolled-{theme}.png"), 400).await;
                scroll_memo(cx, 240.);
                on_tools_window(cx, |window, app, _| hover(window, app, point(px(230.), px(680.))));
            }
        }
    }
    on_page(cx, |_, app, page| page.update(app, |p, cx| p.pose_notes(None, cx)));
    services::save_config(st, |c| c.general.appearance = Appearance::Light);
}

pub async fn run(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    if !services::chat_model_is_local(st) {
        eprintln!("livenotes: set DITO_MOCK_LOCAL_CHAT=1 so the chat model is the local mock");
        return;
    }
    let _nap = dito_platform::system::NapGuard::new("live-notes");
    let original = st.cfg.get();
    services::save_config(st, |c| {
        c.general.appearance = Appearance::Light;
        c.general.display_language = dito_core::config::DisplayLanguage::Ja;
    });
    cx.update(|cx| {
        tools::open(st.clone(), cx);
        resize_tools_window(cx, 1120., 720.);
    });
    let mut audio = vec![0.0; 8_000];
    let mut lines = Vec::new();
    for (i, (text, secs, pause)) in crate::dev::seed::FILM_SCRIPT.iter().enumerate() {
        audio.extend(tone(i, *secs));
        audio.extend(vec![0.0; (pause * 16_000.) as usize]);
        lines.push((text.to_string(), *secs));
    }
    let asr = ToneTranscriber::new(lines, Duration::from_millis(250), Duration::from_millis(60));
    let handle = {
        let _rt = st.rt.enter();
        transcription::spawn(TranscriptionDeps {
            audio: Arc::new(Talk(audio)),
            detectors: Arc::new(EnergyDetectors),
            engines: Arc::new(FixedEngines::new(
                Arc::new(asr),
                Arc::new(MockTextModel::default().with_delay(Duration::from_millis(300))),
            )),
            config: st.cfg.clone(),
            store: st.store.clone(),
        })
    };
    on_tools_window(cx, |w, cx, view| {
        let handle = handle.clone();
        view.update(cx, |t, cx| {
            t.assume_transcription_ready(true, cx);
            t.follow_transcription(handle, w, cx)
        });
    });
    handle.start(Sources { system: true, microphone: false });
    settle(cx, 1_500).await;
    let id = handle.status().state.transcript_id().expect("a running session has its meeting");

    // Show the transcript before opening the panel.
    shot(cx, dir, "livenotes-closed.png", 8_000).await;

    // Opening: frames while the panel slides in, at the design's 0, 50 and 240 ms.
    on_tools_window(cx, |w, app, _| click(w, app, point(px(BOOK_BUTTON.0), px(BOOK_BUTTON.1))));
    let begun = Instant::now();
    for ms in [0u64, 50, 120, 240, 400] {
        settle(cx, Duration::from_millis(ms).saturating_sub(begun.elapsed()).as_millis() as u64).await;
        if let Some(Ok(img)) = render_tools_window(cx) {
            save(dir, &format!("livenotes-open-{ms:03}.png"), img);
        }
    }
    shot(cx, dir, "livenotes-open-light.png", 3_000).await;

    services::save_config(st, |c| c.general.appearance = Appearance::Dark);
    shot(cx, dir, "livenotes-open-dark.png", 800).await;
    services::save_config(st, |c| c.general.appearance = Appearance::Light);

    // Closing reverses the motion; pressing again partway turns it around.
    on_tools_window(cx, |w, app, _| click(w, app, point(px(BOOK_BUTTON.0), px(BOOK_BUTTON.1))));
    shot(cx, dir, "livenotes-closing-120.png", 120).await;
    settle(cx, 600).await;
    shot(cx, dir, "livenotes-closed-again.png", 0).await;
    on_tools_window(cx, |w, app, _| click(w, app, point(px(BOOK_BUTTON.0), px(BOOK_BUTTON.1))));
    settle(cx, 600).await;

    // A question asked in the panel, and the answer in place of the notes.
    on_page(cx, |w, app, page| page.update(app, |p, cx| p.pose_ask("決定したことは？", w, cx)));
    shot(cx, dir, "livenotes-quick-answer.png", 6_000).await;

    // Back to the notes keeps the conversation; the next question continues it.
    on_tools_window(cx, |w, app, _| click(w, app, point(px(BACK_TO_NOTES.0), px(BACK_TO_NOTES.1))));
    shot(cx, dir, "livenotes-back-to-notes.png", 400).await;
    on_page(cx, |w, app, page| page.update(app, |p, cx| p.pose_ask("期限や担当者は？", w, cx)));
    shot(cx, dir, "livenotes-second-question.png", 4_000).await;

    // The conversation goes on in Chat with the meeting still attached.
    on_tools_window(cx, |w, app, _| click(w, app, point(px(CONTINUE_IN_CHAT.0), px(CONTINUE_IN_CHAT.1))));
    on_chat(cx, move |_, app, chat| {
        chat.update(app, |p, cx| p.pose_recording(Some(ActiveRecording { id, phase: RecordingPhase::Recording }), cx))
    });
    shot(cx, dir, "livenotes-chat-continued.png", 1_500).await;

    // Another conversation, with the way back above it, and the meeting attached from the picker's row.
    on_chat(cx, |_, app, chat| chat.update(app, |p, cx| p.pose_open(None, cx)));
    on_chat(cx, move |w, app, chat| {
        chat.update(app, |p, cx| {
            p.pose_link_meeting(id, true, cx);
            p.pose_type("ここまでの流れを三行でまとめて。", w, cx);
        })
    });
    shot(cx, dir, "livenotes-chat-attached.png", 800).await;
    on_chat(cx, |w, app, chat| chat.update(app, |p, cx| p.pose_picker(w, cx)));
    shot(cx, dir, "livenotes-chat-picker.png", 600).await;
    on_chat(cx, |w, app, chat| chat.update(app, |p, cx| p.pose_reset(w, cx)));
    on_chat(cx, move |_, app, chat| {
        chat.update(app, |p, cx| p.pose_recording(Some(ActiveRecording { id, phase: RecordingPhase::Paused }), cx))
    });
    shot(cx, dir, "livenotes-chat-paused.png", 400).await;
    on_chat(cx, move |_, app, chat| {
        chat.update(app, |p, cx| p.pose_recording(Some(ActiveRecording { id, phase: RecordingPhase::Recording }), cx))
    });

    // The way back: one click on the capsule, and the page is as it was left.
    on_tools_window(cx, |w, app, _| click(w, app, point(px(RETURN_CAPSULE.0), px(RETURN_CAPSULE.1))));
    shot(cx, dir, "livenotes-returned.png", 800).await;

    // A reply in Markdown: the list, the bold text and the long paragraphs in the panel's width, in both themes.
    cx.update(|cx| resize_tools_window(cx, 1120., 1000.));
    on_page(cx, |_, app, page| page.update(app, |p, cx| p.pose_conversation(MARKDOWN_QUESTION, MARKDOWN_ANSWER, cx)));
    shot(cx, dir, "livenotes-markdown-light.png", 600).await;
    services::save_config(st, |c| c.general.appearance = Appearance::Dark);
    shot(cx, dir, "livenotes-markdown-dark.png", 800).await;
    services::save_config(st, |c| c.general.appearance = Appearance::Light);
    cx.update(|cx| resize_tools_window(cx, 900., 1000.));
    shot(cx, dir, "livenotes-markdown-narrow.png", 800).await;
    on_page(cx, |_, app, page| page.update(app, |p, cx| p.pose_conversation_reset(cx)));
    cx.update(|cx| resize_tools_window(cx, 1120., 720.));

    // A window too narrow for both: notes and transcript take turns.
    cx.update(|cx| resize_tools_window(cx, 900., 720.));
    shot(cx, dir, "livenotes-narrow-notes.png", 800).await;
    on_tools_window(cx, |w, app, _| click(w, app, point(px(NARROW_BOOK_BUTTON.0), px(NARROW_BOOK_BUTTON.1))));
    shot(cx, dir, "livenotes-narrow-transcript.png", 600).await;

    panel_states(st, dir, cx, false).await;

    handle.stop();
    on_chat(cx, |_, app, chat| chat.update(app, |p, cx| p.pose_recording(None, cx)));
    settle(cx, 4_000).await;
    sync(cx, &handle);
    shot(cx, dir, "livenotes-ended.png", 600).await;
    panel_states(st, dir, cx, true).await;

    if let Some((window, _)) = cx.update(|cx| tools::handle(cx)) {
        close_window(cx, window);
    }
    services::save_config(st, |c| {
        c.general.appearance = original.general.appearance;
        c.general.display_language = original.general.display_language;
    });
    let _ = SessionState::Idle;
}
