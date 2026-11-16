//! Frames for the README animations, filmed from the real HUD, Tools window and chat page. Opt-in:
//! `DITO_SHOTS_ONLY=readmefilm`, with `DITO_FILM_LANG` (`ja`, `en`, `zh`, `ko`) and `DITO_FILM_THEME` (`dark`, `light`).
//! Frames are named `<scene>-<milliseconds since the scene began>.png` and assembled by `scripts/readme-assets.py`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::config::{Appearance, DisplayLanguage, HudPosition, RecordMode};
use dito_core::controller::{Command, HudState};
use dito_core::factory::FixedEngines;
use dito_core::i18n::Lang;
use dito_core::provider::mock::{MockTextModel, ToneTranscriber, tone};
use dito_core::transcription::{self, EnergyDetectors, SessionState, Sources, TranscriptionDeps};
use gpui_kit::*;
use image::RgbaImage;

use super::capture::{close_window, render, render_tools_window, resize_tools_window};
use super::input::on_tools_window;
use super::transcript_film::Talk;
use crate::services;
use crate::state::AppState;
use crate::theme::Palette;
use crate::tools;
use crate::ui;

const EDITOR_W: f32 = 720.;
const EDITOR_H: f32 = 340.;
const HUD_WINDOW_H: i64 = 428;
const HUD_WINDOW_W: i64 = 360;
const FRAME: Duration = Duration::from_millis(66);

struct Words {
    lang: Lang,
    display: DisplayLanguage,
    notes: &'static str,
    typed: &'static str,
    greeting: &'static str,
    rough: &'static str,
    polished: &'static str,
    instruction: &'static str,
    meeting: [(&'static str, f32, f32); 4],
    question: &'static str,
    answer: &'static str,
}

fn words(code: &str) -> Words {
    match code {
        "en" => Words {
            lang: Lang::En,
            display: DisplayLanguage::En,
            notes: "Notes",
            typed: "Could we find a time to meet next Wednesday?",
            greeting: "Hi Tanaka,",
            rough: "hey, the doc isnt done yet, maybe tomorrow?",
            polished: "The document isn't ready yet. I'll finish it tomorrow.",
            instruction: "Rewrite this politely",
            meeting: [
                ("Let's start the weekly sync.", 2.4, 0.6),
                ("Today we are checking the release plan.", 3.2, 0.6),
                ("Staging goes out on Thursday.", 2.6, 0.6),
                ("Only the payment tests are left.", 2.8, 0.0),
            ],
            question: "Summarize what we need to do before the next meeting in three points.",
            answer: "Before the next meeting, there are three things to do:\n\n1. **Update the proposal**\n   Add the cost breakdown and the scope of work.\n2. **Line up the prototype checks**\n   Share each owner's results.\n3. **Confirm the next date**\n   Collect candidates and ask the attendees.",
        },
        "ko" => Words {
            lang: Lang::Ko,
            display: DisplayLanguage::Ko,
            notes: "메모",
            typed: "다음 주 수요일에 회의 시간을 잡을 수 있을까요?",
            greeting: "김민수 님,",
            rough: "아, 자료 아직 안 끝났는데 아마 내일쯤 될 것 같아요",
            polished: "자료 정리가 아직 끝나지 않아 내일 완료할 예정입니다.",
            instruction: "정중하게 다시 써 줘",
            meeting: [
                ("주간 회의를 시작하겠습니다.", 2.4, 0.6),
                ("오늘은 출시 계획을 확인하겠습니다.", 3.2, 0.6),
                ("스테이징 환경에는 목요일에 반영할 예정입니다.", 3.4, 0.6),
                ("결제 관련 테스트만 남아 있습니다.", 2.8, 0.0),
            ],
            question: "다음 회의 전까지 할 일을 세 가지로 정리해 줘.",
            answer: "다음 회의 전까지 할 일은 세 가지입니다:\n\n1. **제안서 업데이트**\n   비용 내역과 작업 범위를 추가합니다.\n2. **시제품 점검 항목 정리**\n   담당자별 점검 결과를 공유합니다.\n3. **다음 회의 일정 확정**\n   가능한 날짜를 모아 참석자에게 확인합니다.",
        },
        "zh" => Words {
            lang: Lang::Zh,
            display: DisplayLanguage::Zh,
            notes: "备忘录",
            typed: "下周三方便抽出时间开个会吗？",
            greeting: "田中您好，",
            rough: "那个，资料还没弄完，可能明天吧",
            polished: "资料尚未整理完毕，预计明天上午可以完成。",
            instruction: "改得礼貌一点",
            meeting: [
                ("我们开始今天的例会。", 2.4, 0.6),
                ("今天确认一下发布计划。", 3.0, 0.6),
                ("预发布环境周四上线。", 2.6, 0.6),
                ("只剩支付相关的测试了。", 2.8, 0.0),
            ],
            question: "请把下次会议前要做的事总结成三点。",
            answer: "下次会议前需要完成三件事：\n\n1. **更新提案书**\n   补充费用明细和工作范围。\n2. **统一原型的检查项**\n   各负责人共享检查结果。\n3. **确定下次会议时间**\n   汇总候选时间，并向参会者确认。",
        },
        _ => Words {
            lang: Lang::Ja,
            display: DisplayLanguage::Ja,
            notes: "メモ",
            typed: "来週の水曜日に、お打ち合わせの時間をいただけますか。",
            greeting: "田中さん",
            rough: "えーと、資料まだ終わってなくて、明日になるかもです。",
            polished: "資料の確認が終わっておらず、明日になりそうです。",
            instruction: "丁寧に書き直して",
            meeting: [
                ("では、定例を始めます。", 2.4, 0.6),
                ("今日はリリース計画の確認です。", 3.2, 0.6),
                ("ステージングへの反映は木曜の予定です。", 3.4, 0.6),
                ("決済まわりのテストだけ、まだ残っています。", 3.4, 0.0),
            ],
            question: "次の会議までにやることを3つにまとめて。",
            answer: "次回までの対応は、以下の3つです。\n\n1. **提案書を更新する**\n   費用の内訳と作業範囲を追記します。\n2. **試作の確認項目をそろえる**\n   担当ごとに結果を共有します。\n3. **次回の日程を確定する**\n   候補をまとめ、参加者に確認します。",
        },
    }
}

struct Film {
    dir: PathBuf,
    scene: &'static str,
    start: Instant,
}

impl Film {
    fn new(dir: &Path, scene: &'static str) -> Self {
        Self { dir: dir.to_path_buf(), scene, start: Instant::now() }
    }

    fn push(&self, image: RgbaImage) {
        self.push_tagged(image, "");
    }

    fn push_tagged(&self, image: RgbaImage, tag: &str) {
        let name = format!("{}-{:06}{tag}.png", self.scene, self.start.elapsed().as_millis());
        let _ = image.save(self.dir.join(name));
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Part {
    Plain,
    Selected,
}

struct Editor {
    p: Palette,
    lang: Lang,
    title: &'static str,
    lines: Vec<Vec<(String, Part)>>,
    caret: bool,
}

impl Render for Editor {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        ui::set_ui_language(self.lang);
        let p = self.p;
        let last = self.lines.len().saturating_sub(1);
        let body = self.lines.iter().enumerate().fold(div().flex().flex_col().gap(px(10.)), |col, (i, line)| {
            let row = line.iter().fold(div().flex().flex_row().items_center().h(px(34.)), |row, (text, part)| {
                let seg = div().px(px(2.)).rounded(px(4.)).text_size(px(21.)).text_color(p.text).child(text.clone());
                row.child(if *part == Part::Selected { seg.bg(p.accent_soft) } else { seg })
            });
            let caret = div().w(px(2.)).h(px(24.)).ml(px(1.)).bg(if self.caret && i == last {
                p.accent
            } else {
                ui::transparent()
            });
            col.child(row.child(caret))
        });
        div().size_full().bg(p.bg).p(px(24.)).font_family(ui::cjk_family()).child(
            div()
                .size_full()
                .bg(p.card)
                .border_1()
                .border_color(p.border)
                .rounded(px(16.))
                .p(px(28.))
                .flex()
                .flex_col()
                .gap(px(18.))
                .child(div().text_size(px(14.)).text_color(p.muted).child(self.title))
                .child(body),
        )
    }
}

fn open_editor(cx: &mut AsyncApp, view: Editor) -> Option<WindowHandle<Editor>> {
    cx.update(|cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(EDITOR_W), px(EDITOR_H)), cx))),
            titlebar: None,
            focus: false,
            show: false,
            ..Default::default()
        };
        cx.open_window(options, |_, cx| cx.new(|_| view)).ok()
    })
}

fn set_editor(cx: &mut AsyncApp, window: WindowHandle<Editor>, lines: Vec<Vec<(String, Part)>>, caret: bool) {
    let _ = cx.update(|cx| {
        cx.update_window(window.into(), |_, w, cx| {
            if let Some(view) = w.root::<Editor>().flatten() {
                view.update(cx, |e, cx| {
                    e.lines = lines;
                    e.caret = caret;
                    cx.notify();
                });
            }
        })
    });
}

fn prefix(text: &str, chars: usize) -> String {
    text.chars().take(chars).collect()
}

fn composite(cx: &mut AsyncApp, editor: WindowHandle<Editor>, film: &Film) {
    let Some(mut frame) = render(cx, editor) else { return };
    if let Some(hud) = cx.update(|cx| crate::hud::live_window(cx))
        && let Some(layer) = render(cx, hud)
    {
        let x = (frame.width() as i64 - HUD_WINDOW_W) / 2;
        let y = frame.height() as i64 - HUD_WINDOW_H + 24;
        image::imageops::overlay(&mut frame, &layer, x, y);
    }
    film.push(frame);
}

fn hud_state(st: &Arc<AppState>) -> HudState {
    st.dictation.hud().borrow().clone()
}

async fn pause(cx: &mut AsyncApp, d: Duration) {
    cx.background_executor().timer(d).await;
}

async fn dictation(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp, w: &Words, p: Palette) {
    let Some(editor) = open_editor(cx, Editor { p, lang: w.lang, title: w.notes, lines: vec![vec![]], caret: true })
    else {
        return;
    };
    let film = Film::new(dir, "dictation");
    let mut pressed = false;
    let mut finished = false;
    let mut inserted: Option<Instant> = None;
    while film.start.elapsed() < Duration::from_secs(25) {
        let t = film.start.elapsed();
        if !pressed && t >= Duration::from_millis(500) {
            st.dictation.send(Command::KeyDown);
            st.dictation.send(Command::KeyUp);
            pressed = true;
        }
        if pressed && !finished && t >= Duration::from_millis(3400) {
            st.dictation.send(Command::Finish);
            finished = true;
        }
        if finished && inserted.is_none() && matches!(hud_state(st), HudState::Inserted | HudState::Copied) {
            inserted = Some(Instant::now());
        }
        let typed = inserted.map_or(0, |at| at.elapsed().as_millis() as usize / 28);
        let caret = (t.as_millis() / 500).is_multiple_of(2);
        set_editor(cx, editor, vec![vec![(prefix(w.typed, typed), Part::Plain)]], caret);
        composite(cx, editor, &film);
        if inserted.is_some_and(|at| at.elapsed() > Duration::from_millis(2400)) {
            break;
        }
        pause(cx, FRAME).await;
    }
    close_window(cx, editor);
    st.dictation.send(Command::Dismiss);
}

async fn command(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp, w: &Words, p: Palette) {
    let lines = |selected: bool| {
        let part = if selected { Part::Selected } else { Part::Plain };
        vec![
            vec![(w.greeting.to_string(), Part::Plain)],
            vec![((if selected { w.rough } else { w.polished }).to_string(), part)],
        ]
    };
    let Some(editor) = open_editor(cx, Editor { p, lang: w.lang, title: w.notes, lines: lines(true), caret: false })
    else {
        return;
    };
    let film = Film::new(dir, "command");
    let mut pressed = false;
    let mut finished = false;
    let mut replaced: Option<Instant> = None;
    let mut chose = false;
    while film.start.elapsed() < Duration::from_secs(25) {
        let t = film.start.elapsed();
        if !pressed && t >= Duration::from_millis(900) {
            st.dictation.send(Command::CommandKeyDown);
            pressed = true;
        }
        if pressed && !finished && t >= Duration::from_millis(3600) {
            st.dictation.send(Command::CommandKeyUp);
            st.dictation.send(Command::Finish);
            finished = true;
        }
        if finished && replaced.is_none() && matches!(hud_state(st), HudState::Replaced) {
            replaced = Some(Instant::now());
        }
        if finished && !chose && t >= Duration::from_millis(6000) && matches!(hud_state(st), HudState::Choosing { .. })
        {
            st.dictation.send(Command::Choose(dito_core::intent::Intent::Rewrite));
            chose = true;
        }
        set_editor(cx, editor, lines(replaced.is_none()), false);
        composite(cx, editor, &film);
        if replaced.is_some_and(|at| at.elapsed() > Duration::from_millis(2400)) {
            break;
        }
        pause(cx, FRAME).await;
    }
    close_window(cx, editor);
    st.dictation.send(Command::Dismiss);
}

async fn meeting(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp, w: &Words) {
    let _nap = dito_platform::system::NapGuard::new("readme-film");
    cx.update(|cx| {
        tools::open(st.clone(), cx);
        resize_tools_window(cx, 960., 600.);
    });
    let mut audio = vec![0.0; 8_000];
    let mut lines = Vec::new();
    for (i, (text, secs, gap)) in w.meeting.iter().enumerate() {
        audio.extend(tone(i, *secs));
        audio.extend(vec![0.0; (gap * 16_000.) as usize]);
        lines.push((text.to_string(), *secs));
    }
    let length = Duration::from_secs_f32(audio.len() as f32 / 16_000.);
    let asr = ToneTranscriber::new(lines, Duration::from_millis(250), Duration::from_millis(60));
    let handle = {
        let _rt = st.rt.enter();
        transcription::spawn(TranscriptionDeps {
            audio: Arc::new(Talk(audio)),
            detectors: Arc::new(EnergyDetectors),
            engines: Arc::new(FixedEngines::new(
                Arc::new(asr),
                Arc::new(MockTextModel::default().with_delay(Duration::from_millis(900))),
            )),
            config: st.cfg.clone(),
            store: st.store.clone(),
        })
    };
    on_tools_window(cx, |win, app, view| {
        let handle = handle.clone();
        view.update(app, |t, cx| {
            t.assume_transcription_ready(true, cx);
            t.follow_transcription(handle, win, cx);
        });
    });
    on_tools_window(cx, |_, app, view| view.update(app, |t, cx| t.pose_transcription_start_pressed(cx)));
    handle.start(Sources { system: true, microphone: false });

    let film = Film::new(dir, "meeting");
    let (arm_at, stop_at) = (length.saturating_sub(Duration::from_millis(1800)), length + Duration::from_millis(200));
    let (mut armed, mut stopped) = (false, false);
    while film.start.elapsed() < Duration::from_secs(40) {
        let t = film.start.elapsed();
        if !armed && t >= arm_at {
            on_tools_window(cx, |_, app, view| view.update(app, |t, cx| t.pose_transcription_stop_armed(cx)));
            armed = true;
        }
        if !stopped && t >= stop_at {
            handle.stop();
            stopped = true;
        }
        let status = handle.status();
        on_tools_window(cx, |win, app, view| view.update(app, |t, cx| t.sync_transcription(status, win, cx)));
        let state = handle.status().state;
        let tag = match state {
            SessionState::Idle => "-idle",
            SessionState::Starting => "-start",
            SessionState::Finishing { .. } => "-fin",
            SessionState::Recording { .. } => "-rec",
        };
        if let Some(Ok(image)) = render_tools_window(cx) {
            film.push_tagged(image, tag);
        }
        if stopped && state == SessionState::Idle {
            break;
        }
        pause(cx, FRAME).await;
    }
    if let Some((window, _)) = cx.update(|cx| tools::handle(cx)) {
        close_window(cx, window);
    }
}

fn on_chat(cx: &mut AsyncApp, f: impl FnOnce(&mut Window, &mut App, &Entity<tools::chat::ChatPage>) + 'static) {
    on_tools_window(cx, |win, app, view| {
        let chat = view.read(app).chat().clone();
        f(win, app, &chat);
    });
}

async fn chat(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp, w: &Words) {
    services::save_config(st, |c| {
        c.text_model.source = dito_core::config::Source::Local;
        c.text_model.local_model = "qwen3.5-4b-instruct".into();
    });
    cx.update(|cx| {
        tools::open(st.clone(), cx);
        resize_tools_window(cx, 960., 600.);
    });
    on_tools_window(cx, |_, app, view| view.update(app, |t, cx| t.show_chat(cx)));
    on_chat(cx, |_, app, chat| chat.update(app, |p, cx| p.pose_open(None, cx)));
    pause(cx, Duration::from_millis(600)).await;
    let film = Film::new(dir, "chat");
    let snap = |cx: &mut AsyncApp| {
        if let Some(Ok(image)) = render_tools_window(cx) {
            film.push(image);
        }
    };
    for _ in 0..8 {
        snap(cx);
        pause(cx, FRAME).await;
    }
    let question_len = w.question.chars().count();
    for n in (1..=question_len).step_by(2).chain(std::iter::once(question_len)) {
        let text = prefix(w.question, n);
        on_chat(cx, move |win, app, chat| chat.update(app, |p, cx| p.pose_type(&text, win, cx)));
        snap(cx);
        pause(cx, FRAME).await;
    }
    for _ in 0..6 {
        snap(cx);
        pause(cx, FRAME).await;
    }
    let answer_len = w.answer.chars().count();
    let sent = Instant::now();
    loop {
        let shown = (sent.elapsed().as_millis() as usize / 22).min(answer_len);
        let (question, written) = (w.question, prefix(w.answer, shown));
        let state = if shown < answer_len {
            dito_core::store::MessageState::Generating
        } else {
            dito_core::store::MessageState::Done
        };
        let ms = sent.elapsed().as_millis() as u64;
        on_chat(cx, move |win, app, chat| {
            chat.update(app, |p, cx| {
                p.pose_open(None, cx);
                p.pose_reset(win, cx);
                p.pose_reply(question, &written, state, ms, cx);
            })
        });
        snap(cx);
        if shown >= answer_len {
            break;
        }
        pause(cx, FRAME).await;
    }
    for _ in 0..30 {
        pause(cx, FRAME).await;
        snap(cx);
    }
    if let Some((window, _)) = cx.update(|cx| tools::handle(cx)) {
        close_window(cx, window);
    }
}

pub async fn run(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let code = std::env::var("DITO_FILM_LANG").unwrap_or_else(|_| "ja".into());
    let appearance =
        if std::env::var("DITO_FILM_THEME").is_ok_and(|t| t == "light") { Appearance::Light } else { Appearance::Dark };
    let w = words(&code);
    let palette = if appearance == Appearance::Light { Palette::light() } else { Palette::dark() };
    services::save_config(st, |c| {
        c.general.display_language = w.display;
        c.general.appearance = appearance;
        c.general.hud_appearance = appearance;
        c.general.hud_position = HudPosition::Centered;
        c.general.record_mode = RecordMode::Toggle;
        c.general.confirm_before_replace = false;
    });
    cx.update(|cx| crate::hud::run(st.clone(), cx));
    let only = std::env::var("DITO_FILM_SCENES").unwrap_or_else(|_| "dictation,command,meeting,chat".into());
    let wants = |scene: &str| only.split(',').any(|s| s == scene);
    if let Some(server) = &st._mock_server {
        server.behavior(|b| b.latency = Duration::from_millis(900));
    }
    if wants("dictation") {
        dictation(st, dir, cx, &w, palette).await;
    }
    if wants("command") {
        if let Some(server) = &st._mock_server {
            let instruction = w.instruction.to_string();
            server.behavior(move |b| b.transcript = instruction);
        }
        command(st, dir, cx, &w, palette).await;
    }
    if wants("meeting") {
        meeting(st, dir, cx, &w).await;
    }
    if wants("chat") {
        chat(st, dir, cx, &w).await;
    }
}
