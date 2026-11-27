//! The live meeting notes through the real session actor, page and notes runner (`design:tools/recording/live-notes`):
//! a scripted meeting is recorded while the panel opens and closes, a question is asked and handed to Chat, and the
//! way back from Chat is taken. The chat model is the rule-based mock (`DITO_MOCK_LOCAL_CHAT=1`), so no model is
//! needed. Opt-in: `DITO_SHOTS_ONLY=livenotes`.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::config::Appearance;
use dito_core::factory::FixedEngines;
use dito_core::provider::mock::{MockTextModel, ToneTranscriber, tone};
use dito_core::transcription::{
    self, ActiveRecording, EnergyDetectors, RecordingPhase, SessionState, Sources, TranscriptionDeps,
    TranscriptionHandle,
};
use gpui_kit::*;

use super::capture::{close_window, render_tools_window, resize_tools_window, save};
use super::input::{click, on_tools_window};
use super::transcript_film::Talk;
use crate::services;
use crate::state::AppState;
use crate::tools;

async fn settle(cx: &mut AsyncApp, ms: u64) {
    cx.background_executor().timer(Duration::from_millis(ms)).await;
}

async fn shot(cx: &mut AsyncApp, dir: &Path, name: &str, wait: u64) {
    settle(cx, wait).await;
    sync(cx, None);
    if let Some(Ok(img)) = render_tools_window(cx) {
        save(dir, name, img);
    }
}

/// Hands the page the session's status at once, as its subscription would between frames.
fn sync(cx: &mut AsyncApp, handle: Option<&TranscriptionHandle>) {
    let Some(handle) = handle else { return };
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

/// Buttons' centres on the 1120 by 720 window, found by looking at the shots.
const BOOK_BUTTON: (f32, f32) = (1045., 67.);
const CONTINUE_IN_CHAT: (f32, f32) = (1032., 131.);
const BACK_TO_NOTES: (f32, f32) = (958., 131.);
const RETURN_CAPSULE: (f32, f32) = (540., 54.);
/// The book button once the window is 900 wide.
const NARROW_BOOK_BUTTON: (f32, f32) = (826., 67.);

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

    // Nothing there yet: the panel opens on an empty transcript and waits.
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

    // A window too narrow for both: notes and transcript take turns.
    cx.update(|cx| resize_tools_window(cx, 900., 720.));
    shot(cx, dir, "livenotes-narrow-notes.png", 800).await;
    on_tools_window(cx, |w, app, _| click(w, app, point(px(NARROW_BOOK_BUTTON.0), px(NARROW_BOOK_BUTTON.1))));
    shot(cx, dir, "livenotes-narrow-transcript.png", 600).await;

    handle.stop();
    on_chat(cx, |_, app, chat| chat.update(app, |p, cx| p.pose_recording(None, cx)));
    settle(cx, 4_000).await;
    sync(cx, Some(&handle));
    shot(cx, dir, "livenotes-ended.png", 600).await;

    if let Some((window, _)) = cx.update(|cx| tools::handle(cx)) {
        close_window(cx, window);
    }
    services::save_config(st, |c| {
        c.general.appearance = original.general.appearance;
        c.general.display_language = original.general.display_language;
    });
    let _ = SessionState::Idle;
}
