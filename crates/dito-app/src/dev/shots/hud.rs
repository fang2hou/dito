use std::cell::Cell;
use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::config::{Appearance, DisplayLanguage, HudPosition, PreferredLanguage, RecordMode};
use dito_core::controller::{Choice, Command, ErrorAction, HudState, ResultWrite};
use dito_core::error::ErrorKind;
use dito_core::intent::Intent;
use gpui_kit::*;
use image::RgbaImage;

use super::input;

use super::capture::{
    BACKDROP_DARK, BACKDROP_LIGHT, BACKDROP_WHITE, close_window, hud_view, open_hud_window, over_backdrop, render,
    save, theme_tag, wait_for_frame,
};
use crate::hud::{HUD_SCALE, HudView, PreviewHover, WIN_PAD};
use crate::services;
use crate::state::AppState;
use crate::ui::Root;

struct HudCase {
    name: &'static str,
    state: HudState,
    seconds_in: f32,
    hover: PreviewHover,
    came_from: Option<HudState>,
    /// A run started by the command shortcut.
    command: bool,
    /// Typed into the instruction field or the text window once it is open.
    typed: Option<&'static str>,
}

impl HudCase {
    fn new(name: &'static str, state: HudState, seconds_in: f32) -> Self {
        Self { name, state, seconds_in, hover: PreviewHover::Away, came_from: None, command: false, typed: None }
    }

    fn command(name: &'static str, state: HudState, seconds_in: f32) -> Self {
        Self { command: true, ..Self::new(name, state, seconds_in) }
    }

    fn typed(mut self, text: &'static str) -> Self {
        self.typed = Some(text);
        self
    }

    fn hovering(mut self, hover: PreviewHover) -> Self {
        self.hover = hover;
        self
    }

    fn after_hold(mut self) -> Self {
        self.came_from = Some(HudState::Listening { mode: RecordMode::Hold });
        self
    }
}

const RESULT: &str = "お疲れさまです。ご相談の件、来週水曜までに資料をお送りします。ご確認をお願いします。";

fn choosing() -> HudState {
    let choice = |intent, percent| Choice { intent, percent };
    HudState::Choosing {
        choices: vec![choice(Intent::Summarize, 62), choice(Intent::Translate, 27), choice(Intent::Rewrite, 11)],
    }
}

fn hud_cases() -> Vec<HudCase> {
    let toggle = || HudState::Listening { mode: RecordMode::Toggle };
    let hold = || HudState::Listening { mode: RecordMode::Hold };
    let error = |kind, action| HudState::Error { kind, detail: String::new(), action };
    vec![
        HudCase::new("listening", toggle(), 1.0),
        HudCase::new("countdown", toggle(), 240.2),
        HudCase::new("countdown-warning", toggle(), 290.0),
        HudCase::new("listening-hover", toggle(), 1.0).hovering(PreviewHover::Zone),
        HudCase::new("listening-lang", toggle(), 1.0).hovering(PreviewHover::Zone),
        HudCase::new("hover-finish", toggle(), 1.0).hovering(PreviewHover::Action),
        HudCase::new("hover-cancel", toggle(), 1.0).hovering(PreviewHover::Dismiss),
        HudCase::new("hold", hold(), 1.0),
        HudCase::new("hold-countdown", hold(), 240.2),
        HudCase::new("hold-release-0", HudState::Transcribing, 0.0).after_hold(),
        HudCase::new("hold-release-mid", HudState::Transcribing, 0.07).after_hold(),
        HudCase::new("hold-release-end", HudState::Transcribing, 0.25).after_hold(),
        HudCase::new("transcribing", HudState::Transcribing, 1.48),
        HudCase::new("transcribing-hover", HudState::Transcribing, 1.48).hovering(PreviewHover::Zone),
        HudCase::new("polishing", HudState::Polishing, 1.03),
        HudCase::new("polishing-late", HudState::Polishing, 5.0),
        HudCase::new("transcribing-early", HudState::Transcribing, 0.15),
        HudCase::new("inserted", HudState::Inserted, 0.0),
        HudCase::new("copied", HudState::Copied, 0.0),
        HudCase::new("error-auth", error(ErrorKind::Auth, ErrorAction::Retry), 0.0),
        HudCase::new("error-mic", error(ErrorKind::Mic, ErrorAction::OpenSettings), 0.0),
        HudCase::new("error-retry-hover", error(ErrorKind::NoSpeech, ErrorAction::Retry), 0.0)
            .hovering(PreviewHover::Action),
        HudCase::new("error-settings-hover", error(ErrorKind::Auth, ErrorAction::OpenSettings), 0.0)
            .hovering(PreviewHover::Action),
        HudCase::new("error-close-hover", error(ErrorKind::Mic, ErrorAction::OpenSettings), 0.0)
            .hovering(PreviewHover::Dismiss),
        HudCase::new("copy-flow", HudState::Copied, 0.47),
        HudCase::new("copy-overshoot", HudState::Copied, 0.56),
        HudCase::new("copy-settled", HudState::Copied, 0.90),
        HudCase::command("cmd-listening", toggle(), 1.0),
        HudCase::command("cmd-listening-hover", toggle(), 1.0).hovering(PreviewHover::Zone),
        HudCase::command("cmd-hover-finish", toggle(), 1.0).hovering(PreviewHover::Action),
        HudCase::command("cmd-transcribing", HudState::Transcribing, 1.48),
        HudCase::command("cmd-thinking", HudState::Thinking, 1.0),
        HudCase::command("cmd-working", HudState::Working, 1.2),
        HudCase::command("cmd-replaced", HudState::Replaced, 0.0),
        HudCase::command("cmd-not-recognized", HudState::NotRecognized, 0.0),
        HudCase::command("cmd-choosing", choosing(), 1.0),
        HudCase::command("cmd-typing-empty", HudState::TypingInstruction, 1.0),
        HudCase::command("cmd-typing-one", HudState::TypingInstruction, 1.0).typed("敬語に直して"),
        HudCase::command("cmd-typing-two", HudState::TypingInstruction, 1.0)
            .typed("敬語に直して、最後に日程の確認を一文足して"),
        HudCase::command("cmd-typing-three", HudState::TypingInstruction, 1.0)
            .typed("敬語に直して、最後に日程の確認を一文足して。相手は取引先の部長です。\nできるだけ短く、三行で"),
        HudCase::command("cmd-needs-text-empty", HudState::NeedsText, 1.0),
        HudCase::command("cmd-needs-text", HudState::NeedsText, 1.0).typed(
            "お疲れさまです。先日ご相談した件ですが、来週の水曜日までに資料をまとめてお送りしますので、ご確認いただけますと幸いです。",
        ),
        HudCase::command("cmd-answer", HudState::Answer { text: RESULT.into() }, 1.0),
        HudCase::command("cmd-confirm", HudState::Confirm { text: RESULT.into(), write: ResultWrite::ReplaceSelection }, 1.0),
        HudCase::command(
            "cmd-confirm-reply",
            HudState::Confirm { text: RESULT.into(), write: ResultWrite::CopyOnly },
            1.0,
        ),
        HudCase::command(
            "cmd-confirm-long",
            HudState::Confirm { text: RESULT.repeat(8), write: ResultWrite::ReplaceSelection },
            1.0,
        ),
    ]
}

pub async fn hud_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    // `DITO_HUD_CASES=cmd-` renders only the cases whose name starts with it.
    let only = std::env::var("DITO_HUD_CASES").ok();
    for case in hud_cases().into_iter().filter(|c| only.as_deref().is_none_or(|p| c.name.starts_with(p))) {
        for dark in [true, false] {
            let appearance = if dark { Appearance::Dark } else { Appearance::Light };
            services::save_config(st, |c| c.general.hud_appearance = appearance);
            let (st, state) = (st.clone(), case.state.clone());
            let (seconds, hover, came_from) = (case.seconds_in, case.hover, case.came_from.clone());
            let command = case.command;
            let opened = open_hud_window(cx, move |_| {
                let view = HudView::preview(st, state, seconds, hover, came_from);
                if command { view.commanded() } else { view }
            });
            let Some(window) = opened else { continue };
            cx.background_executor().timer(Duration::from_millis(350)).await;
            if let Some(typed) = case.typed {
                let view = hud_view(cx, window).expect("HUD view");
                let _ = cx.update(|cx| {
                    cx.update_window(window.into(), |_, w, cx| view.update(cx, |v, cx| v.preview_text(typed, w, cx)))
                });
                cx.background_executor().timer(Duration::from_millis(350)).await;
            }
            if case.name == "listening-lang" {
                let _ = cx.update(|cx| {
                    cx.update_window(window.into(), |_, w, cx| input::hover(w, cx, point(px(95.), px(102.))))
                });
                cx.background_executor().timer(Duration::from_millis(200)).await;
            }
            if let Some(img) = render(cx, window) {
                let backdrop = if dark { BACKDROP_DARK } else { BACKDROP_LIGHT };
                save(dir, &format!("hud-{}-{}.png", case.name, theme_tag(appearance)), over_backdrop(&img, backdrop));
            }
            close_window(cx, window);
        }
    }
}

pub async fn hud_outcome_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    for (appearance, backdrop) in [(Appearance::Light, BACKDROP_WHITE), (Appearance::Dark, BACKDROP_DARK)] {
        let tag = theme_tag(appearance);
        services::save_config(st, |c| c.general.hud_appearance = appearance);
        for (copy, name) in [(false, "check"), (true, "copy")] {
            let st = st.clone();
            let window = open_hud_window(cx, move |cx| HudView::outcome_preview(st, copy, cx)).expect("HUD window");
            let start = Instant::now();
            let mut timestamps = String::new();
            for frame in 0..if copy { 66 } else { 54 } {
                wait_for_frame(cx, start, frame).await;
                let img = render(cx, window).expect("outcome frame");
                timestamps.push_str(&format!("{frame}\t{}\n", start.elapsed().as_secs_f64()));
                save(dir, &format!("outcome-{tag}-{name}-{frame:03}.png"), over_backdrop(&img, backdrop));
            }
            std::fs::write(dir.join(format!("outcome-{tag}-{name}-times.tsv")), timestamps).expect("write frame times");
            close_window(cx, window);
        }
    }
}

pub async fn hud_wave_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    for (appearance, backdrop) in [(Appearance::Light, BACKDROP_WHITE), (Appearance::Dark, BACKDROP_DARK)] {
        let tag = theme_tag(appearance);
        services::save_config(st, |c| c.general.hud_appearance = appearance);
        for (gain, volume) in [(0.0006, "quiet"), (0.06, "normal")] {
            let st = st.clone();
            let window =
                open_hud_window(cx, move |cx| HudView::waveform_preview(st, gain, cx)).expect("native waveform window");
            let start = Instant::now();
            let mut frames = HashSet::new();
            let mut timestamps = String::new();
            for frame in 0..120 {
                wait_for_frame(cx, start, frame).await;
                let img = render(cx, window).expect("native waveform frame");
                let canvas = over_backdrop(&img, backdrop);
                let (w, h) = canvas.dimensions();
                frames.insert(image::imageops::crop_imm(&canvas, w / 2 - 20, h - 65, 40, 40).to_image().into_raw());
                timestamps.push_str(&format!("{frame}\t{}\n", start.elapsed().as_secs_f64()));
                save(dir, &format!("wave-{tag}-{volume}-{frame:03}.png"), canvas);
            }
            if !dito_platform::system::reduce_motion() {
                assert!(
                    frames.len() > 10,
                    "{tag}/{volume}: waveform did not follow input ({} distinct frames)",
                    frames.len()
                );
            }
            let _ = std::fs::write(dir.join(format!("wave-{tag}-{volume}-times.tsv")), timestamps);
            println!("native waveform verified: {tag}/{volume}, {} distinct speaking frames", frames.len());
            close_window(cx, window);
        }
    }
}

/// The real, continuously ticking native renderer: no frozen timestamps or HTML animation.
pub async fn hud_motion_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    for (appearance, backdrop) in [(Appearance::Light, BACKDROP_LIGHT), (Appearance::Dark, BACKDROP_DARK)] {
        let tag = theme_tag(appearance);
        services::save_config(st, |c| c.general.hud_appearance = appearance);
        let st = st.clone();
        let Some(window) = open_hud_window(cx, move |cx| HudView::motion_preview(st, cx)) else { continue };
        let start = Instant::now();
        let mut timestamps = String::new();
        let mut held_labels = HashSet::new();
        let mut crisp_brush = false;
        for frame in 0..120 {
            wait_for_frame(cx, start, frame).await;
            let Some(img) = render(cx, window) else { continue };
            let canvas = over_backdrop(&img, backdrop);
            if start.elapsed().as_secs_f32() >= 1.6 {
                held_labels.insert(held_label_sample(&canvas));
                crisp_brush |= has_crisp_brush(&canvas, appearance);
            }
            save(dir, &format!("hud-motion-{tag}-{frame:03}.png"), canvas);
            timestamps.push_str(&format!("{frame}\t{}\n", start.elapsed().as_secs_f64()));
        }
        assert!(crisp_brush, "{tag}: progress front was lost in its halo");
        if !dito_platform::system::reduce_motion() {
            assert!(held_labels.len() > 10, "{tag}: text sweep did not animate after progress stopped");
        }
        println!("native motion verified: {tag}, crisp brush, {} distinct held-fill label frames", held_labels.len());
        let _ = std::fs::write(dir.join(format!("hud-motion-{tag}-times.tsv")), timestamps);
        close_window(cx, window);
    }
}

fn scaled(points: f32) -> u32 {
    (points * HUD_SCALE).round() as u32
}

/// The window keeps this much room for the countdown under the capsule.
const BELOW_CAPSULE: f32 = 30.;

fn held_label_sample(canvas: &RgbaImage) -> Vec<u8> {
    let (w, h) = canvas.dimensions();
    let top = h - scaled(BELOW_CAPSULE + 39.);
    image::imageops::crop_imm(canvas, w / 2 - scaled(18.), top, scaled(36.), scaled(18.)).to_image().into_raw()
}

/// At this row the label cannot contribute: a crisp front must remain visibly stronger than its low-opacity halo
/// in both themes.
fn has_crisp_brush(canvas: &RgbaImage, appearance: Appearance) -> bool {
    let (w, h) = canvas.dimensions();
    (w / 2 - scaled(6.)..w / 2 + scaled(6.)).any(|x| {
        let red = canvas.get_pixel(x, h - scaled(BELOW_CAPSULE + 22.))[0];
        if appearance == Appearance::Dark { red > 150 } else { red < 200 }
    })
}

/// In design points: the opaque run of the row through the capsule's middle.
fn capsule_width_pt(img: &RgbaImage) -> f32 {
    let y = (img.height() as f32 - (35. + WIN_PAD + 16.5) * HUD_SCALE).round() as u32;
    let opaque: Vec<u32> = (0..img.width()).filter(|&x| img.get_pixel(x, y)[3] >= 250).collect();
    match (opaque.first(), opaque.last()) {
        (Some(a), Some(b)) => (b - a + 1) as f32 / HUD_SCALE,
        _ => 0.,
    }
}

fn count_display_frames(window: &Window, frames: Rc<Cell<usize>>, until: Instant) {
    window.on_next_frame(move |window, _| {
        if Instant::now() < until {
            frames.set(frames.get() + 1);
            count_display_frames(window, frames, until);
        }
    });
}

/// Drives the live HUD window, not a frozen preview. Opt-in: `DITO_SHOTS_ONLY=hudlive`.
pub async fn hud_live_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let before = st.cfg.get();
    services::save_config(st, |c| {
        c.general.record_mode = RecordMode::Hold;
        c.general.hud_position = HudPosition::Centered;
    });
    cx.update(|cx| crate::hud::run(st.clone(), cx));
    st.dictation.send(Command::KeyDown);
    cx.background_executor().timer(Duration::from_millis(120)).await;
    let window = cx.update(|cx| crate::hud::live_window(cx)).expect("live mock HUD");
    let bounds = |cx: &mut AsyncApp| cx.update(|cx| cx.update_window(window, |_, w, _| w.bounds())).ok();
    let start = bounds(cx);
    services::save_config(st, |c| c.general.hud_position = HudPosition::Pointer);
    cx.background_executor().timer(Duration::from_millis(240)).await;
    println!("live HUD pointer-follow bounds: {start:?} -> {:?}", bounds(cx));
    let frames = Rc::new(Cell::new(0));
    let duration = Duration::from_millis(500);
    let (active, visible, refresh) = cx
        .update(|cx| {
            cx.update_window(window, |_, w, app| {
                count_display_frames(w, frames.clone(), Instant::now() + duration);
                (w.is_window_active(), w.is_visible(), w.refresh_interval(app))
            })
        })
        .expect("live HUD display clock");
    cx.background_executor().timer(duration).await;
    println!(
        "live HUD display callbacks: {:.0}/s, active={active}, visible={visible}, interval={refresh:?}",
        frames.get() as f64 / duration.as_secs_f64()
    );
    let (opened_at, hold_for) = (Instant::now(), Duration::from_millis(1000));
    let mut released_at: Option<Instant> = None;
    let mut n = 0;
    while opened_at.elapsed() < hold_for + Duration::from_millis(900) {
        if released_at.is_none() && opened_at.elapsed() >= hold_for {
            st.dictation.send(Command::KeyUp);
            released_at = Some(Instant::now());
        }
        if let Some(img) = render(cx, window) {
            let since_open = opened_at.elapsed().as_millis();
            let since_release = released_at.map(|r| r.elapsed().as_millis() as i128).unwrap_or(-1);
            println!(
                "hud-live-hold frame {n:2}: +{since_open:4} ms open, {since_release:4} ms since release, capsule {:.1} pt",
                capsule_width_pt(&img)
            );
            if n % 3 == 0 {
                save(dir, &format!("hud-live-hold-{n:02}.png"), over_backdrop(&img, BACKDROP_DARK));
            }
            n += 1;
        }
        cx.background_executor().timer(Duration::from_millis(20)).await;
    }
    services::save_config(st, |c| {
        c.general.record_mode = before.general.record_mode;
        c.general.hud_position = before.general.hud_position;
    });
}

/// Drives a whole command through the live HUD window, with the mock selection and router: choice, confirm, replace.
/// Opt-in: `DITO_SHOTS_ONLY=hudcommand`.
pub async fn hud_command_live_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let before = st.cfg.get();
    services::save_config(st, |c| {
        c.general.record_mode = RecordMode::Toggle;
        c.general.hud_position = HudPosition::Centered;
        c.general.hud_appearance = Appearance::Dark;
        c.general.confirm_before_replace = false;
    });
    cx.update(|cx| crate::hud::run(st.clone(), cx));
    let hud = st.dictation.hud();
    let describe = |hud: &tokio::sync::watch::Receiver<HudState>| format!("{:?}", hud.borrow().clone());
    st.dictation.send(Command::CommandKeyDown);
    cx.background_executor().timer(Duration::from_millis(600)).await;
    println!("hudcommand listening: {}", describe(&hud));
    st.dictation.send(Command::CommandKeyUp);
    st.dictation.send(Command::Finish);
    cx.background_executor().timer(Duration::from_millis(1500)).await;
    println!("hudcommand after finish: {}", describe(&hud));
    let window = cx.update(|cx| crate::hud::live_window(cx)).expect("live HUD");
    let live = |cx: &mut AsyncApp, name: &str| {
        if let Some(img) = render(cx, window) {
            save(dir, &format!("hudcommand-{name}.png"), over_backdrop(&img, BACKDROP_DARK));
        }
    };
    live(cx, "1-choosing");
    let click = |cx: &mut AsyncApp, x: f32, y: f32| {
        let _ = cx.update(|cx| cx.update_window(window, |_, w, cx| input::click(w, cx, point(px(x), px(y)))));
    };
    // The likeliest choice: the first button of the row, in the window's own coordinates.
    click(cx, 100., 290.);
    cx.background_executor().timer(Duration::from_millis(1500)).await;
    println!("hudcommand after choosing: {}", describe(&hud));
    live(cx, "2-after-choice");
    let active = cx.update(|cx| cx.update_window(window, |_, w, _| w.is_window_active())).ok();
    println!("hudcommand panel key: {active:?}");
    let regions = cx.update(|cx| {
        let root = window.downcast::<Root>()?;
        let view = root.update(cx, |root, _, _| root.view().clone().downcast::<HudView>().ok()).ok()??;
        // Inside the window, on its action row, in the empty space beside it and on the capsule's row.
        Some(
            [(180., 250.), (140., 105.), (20., 250.), (180., 400.)].map(|(x, y)| view.read(cx).command_region_at(x, y)),
        )
    });
    println!("hudcommand regions (window, action, aside, capsule row): {regions:?}");
    st.dictation.send(Command::Dismiss);
    cx.background_executor().timer(Duration::from_millis(400)).await;

    // The keyboard button: type an instruction, send it with Enter.
    st.dictation.send(Command::CommandKeyDown);
    cx.background_executor().timer(Duration::from_millis(400)).await;
    st.dictation.send(Command::TypeInstruction);
    cx.background_executor().timer(Duration::from_millis(600)).await;
    println!("hudcommand typing: {}", describe(&hud));
    let window = cx.update(|cx| crate::hud::live_window(cx)).expect("live HUD");
    let view = cx.update(|cx| {
        let root = window.downcast::<Root>()?;
        root.update(cx, |root, _, _| root.view().clone().downcast::<HudView>().ok()).ok()?
    });
    if let Some(view) = view {
        let _ = cx.update(|cx| {
            cx.update_window(window, |_, w, cx| view.update(cx, |v, cx| v.preview_text("整形して", w, cx)))
        });
        cx.background_executor().timer(Duration::from_millis(300)).await;
        let key = cx.update(|cx| cx.update_window(window, |_, w, _| w.is_window_active())).ok();
        println!("hudcommand typing panel key: {key:?}");
        let _ = cx.update(|cx| cx.update_window(window, |_, w, cx| input::press_key(w, cx, "enter")));
        cx.background_executor().timer(Duration::from_millis(1500)).await;
        println!("hudcommand after enter: {}", describe(&hud));
    }
    st.dictation.send(Command::Dismiss);
    cx.background_executor().timer(Duration::from_millis(300)).await;
    services::save_config(st, |c| {
        c.general.record_mode = before.general.record_mode;
        c.general.hud_position = before.general.hud_position;
        c.general.hud_appearance = before.general.hud_appearance;
        c.general.confirm_before_replace = before.general.confirm_before_replace;
    });
}

/// Drives the chord hold through the live HUD window: the dictation key down first, then Shift inside the wait, after
/// the wait, or never. Opt-in: `DITO_SHOTS_ONLY=hudchord`. Frames are named by the milliseconds since the first key.
pub async fn hud_chord_live_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let before = st.cfg.get();
    services::save_config(st, |c| {
        c.general.record_mode = RecordMode::Toggle;
        c.general.hud_position = HudPosition::Centered;
        c.general.hud_appearance = Appearance::Dark;
    });
    cx.update(|cx| crate::hud::run(st.clone(), cx));
    let hud = st.dictation.hud();
    // (name, when Shift goes down in ms, or never)
    let cases: [(&str, Option<u64>); 3] = [("inside", Some(60)), ("never", None), ("after", Some(260))];
    for (name, shift_at) in cases {
        let pressed = Instant::now();
        st.dictation.send(Command::KeyDownMayHandOver);
        let mut shifted = false;
        let mut last = None;
        while pressed.elapsed() < Duration::from_millis(700) {
            let ms = pressed.elapsed().as_millis() as u64;
            if !shifted && shift_at.is_some_and(|at| ms >= at) {
                st.dictation.send(Command::KeyToCommand);
                shifted = true;
                println!("hudchord {name}: KeyToCommand sent at {ms} ms");
            }
            let state = hud.borrow().clone();
            let kind = st.dictation.run_kind();
            if last.as_ref() != Some(&(state.clone(), kind)) {
                println!("hudchord {name}: {ms:4} ms hud {state:?}, run {kind:?}");
                last = Some((state, kind));
            }
            let window = cx.update(|cx| crate::hud::live_window(cx));
            if let Some(img) = window.and_then(|window| render(cx, window)) {
                println!("hudchord {name}: {ms:4} ms frame");
                save(dir, &format!("hudchord-{name}-{ms:04}.png"), over_backdrop(&img, BACKDROP_DARK));
            }
            cx.background_executor().timer(Duration::from_millis(8)).await;
        }
        st.dictation.send(Command::Cancel);
        cx.background_executor().timer(Duration::from_millis(500)).await;
    }
    services::save_config(st, |c| {
        c.general.record_mode = before.general.record_mode;
        c.general.hud_position = before.general.hud_position;
        c.general.hud_appearance = before.general.hud_appearance;
    });
}

pub async fn hud_tooltip_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let original = st.cfg.get();
    for dark in [false, true] {
        let appearance = if dark { Appearance::Dark } else { Appearance::Light };
        services::save_config(st, |config| {
            config.general.hud_appearance = appearance;
            config.general.display_language = DisplayLanguage::Ja;
            config.general.hud_languages = PreferredLanguage::ALL.to_vec();
        });
        let preview_st = st.clone();
        let window = open_hud_window(cx, move |_| {
            HudView::preview(preview_st, HudState::Listening { mode: RecordMode::Toggle }, 1., PreviewHover::Zone, None)
        })
        .expect("tooltip HUD");
        cx.background_executor().timer(Duration::from_millis(350)).await;
        for (name, pointer, delay) in [
            ("auto", Some(95.), 220),
            ("expand-early", Some(199.), 20),
            ("expand-middle", None, 60),
            ("long", None, 180),
            ("shrink-early", Some(95.), 20),
            ("shrink-middle", None, 60),
            ("interrupted", Some(129.), 40),
            ("interrupted-again", Some(164.), 40),
            ("settled", None, 220),
            ("leave-grace", Some(20.), 60),
            ("hidden", None, 350),
            ("return", Some(164.), 220),
            ("escape", None, 40),
        ] {
            if name == "escape" {
                let _ = cx.update(|cx| cx.update_window(window.into(), |_, w, cx| input::press_key(w, cx, "escape")));
            }
            if let Some(x) = pointer {
                let _ = cx.update(|cx| {
                    cx.update_window(window.into(), |_, w, cx| {
                        input::hover(w, cx, point(px(x), px(102.)));
                    })
                });
            }
            cx.background_executor().timer(Duration::from_millis(delay)).await;
            if let Some(image) = render(cx, window) {
                save(
                    dir,
                    &format!("tooltip-{}-{name}.png", theme_tag(appearance)),
                    over_backdrop(&image, if dark { BACKDROP_DARK } else { BACKDROP_LIGHT }),
                );
            }
        }
        close_window(cx, window);
    }
    services::save_config(st, |config| {
        config.general.hud_appearance = original.general.hud_appearance;
        config.general.display_language = original.general.display_language;
        config.general.hud_languages = original.general.hud_languages.clone();
    });
}

/// Uses the same pointer poll and render paths as the live non-activating panel.
pub async fn hud_interaction_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let original = st.cfg.get();
    for (appearance, backdrop) in [(Appearance::Light, BACKDROP_LIGHT), (Appearance::Dark, BACKDROP_DARK)] {
        let tag = theme_tag(appearance);
        services::save_config(st, |c| {
            c.general.hud_appearance = appearance;
            c.general.display_language = DisplayLanguage::Ja;
        });
        let preview = st.clone();
        let window = open_hud_window(cx, move |cx| HudView::entry_preview(preview, cx)).expect("entry HUD");
        let start = Instant::now();
        for frame in 0..15 {
            wait_for_frame(cx, start, frame).await;
            let image = render(cx, window).expect("entry frame");
            save(dir, &format!("entry-{tag}-{frame:03}.png"), over_backdrop(&image, backdrop));
        }
        close_window(cx, window);
        let preview = st.clone();
        let window = open_hud_window(cx, move |_| {
            HudView::preview(preview, HudState::Transcribing, 1., PreviewHover::Away, None)
        })
        .expect("Stop HUD");
        for (name, x, y, delay) in [
            ("hidden", 10., 10., 180),
            ("enter", 146.5, 138.5, 70),
            ("shown", 146.5, 138.5, 180),
            ("button-hover", 146.5, 101.5, 180),
            ("gap", 146.5, 119., 100),
            ("leave-grace", 10., 10., 180),
            ("exit-middle", 10., 10., 140),
            ("reenter", 146.5, 138.5, 50),
            ("returned", 146.5, 101.5, 180),
            ("exit", 10., 10., 500),
        ] {
            let view = hud_view(cx, window).expect("HUD view");
            cx.update(|cx| view.update(cx, |view, cx| view.preview_pointer(x, y, cx)));
            cx.background_executor().timer(Duration::from_millis(delay)).await;
            let image = render(cx, window).expect("Stop frame");
            save(dir, &format!("stop-{tag}-{name}.png"), over_backdrop(&image, backdrop));
        }
        close_window(cx, window);
    }
    services::save_config(st, |c| {
        c.general.hud_appearance = original.general.hud_appearance;
        c.general.display_language = original.general.display_language;
    });
}
