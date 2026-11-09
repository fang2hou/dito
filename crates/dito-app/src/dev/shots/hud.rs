use std::cell::Cell;
use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::config::{Appearance, DisplayLanguage, HudPosition, PreferredLanguage, RecordMode};
use dito_core::controller::{Command, ErrorAction, HudState};
use dito_core::error::ErrorKind;
use gpui_kit::*;
use image::RgbaImage;

use super::input;

use super::capture::{
    BACKDROP_DARK, BACKDROP_LIGHT, BACKDROP_WHITE, close_window, open_hud_window, over_backdrop, render, save,
    theme_tag, wait_for_frame,
};
use crate::hud::{HUD_SCALE, HudView, PreviewHover, WIN_PAD};
use crate::services;
use crate::state::AppState;

struct HudCase {
    name: &'static str,
    state: HudState,
    seconds_in: f32,
    hover: PreviewHover,
    came_from: Option<HudState>,
}

impl HudCase {
    fn new(name: &'static str, state: HudState, seconds_in: f32) -> Self {
        Self { name, state, seconds_in, hover: PreviewHover::Away, came_from: None }
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
        HudCase::new("hold-release-0", HudState::Recognizing, 0.0).after_hold(),
        HudCase::new("hold-release-mid", HudState::Recognizing, 0.07).after_hold(),
        HudCase::new("hold-release-end", HudState::Recognizing, 0.25).after_hold(),
        HudCase::new("recognizing", HudState::Recognizing, 1.48),
        HudCase::new("recognizing-hover", HudState::Recognizing, 1.48).hovering(PreviewHover::Zone),
        HudCase::new("polishing", HudState::Polishing, 1.03),
        HudCase::new("polishing-late", HudState::Polishing, 5.0),
        HudCase::new("recognizing-early", HudState::Recognizing, 0.15),
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
    ]
}

pub async fn hud_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    for case in hud_cases() {
        for dark in [true, false] {
            let appearance = if dark { Appearance::Dark } else { Appearance::Light };
            services::save_config(st, |c| c.general.hud_appearance = appearance);
            let (st, state) = (st.clone(), case.state.clone());
            let (seconds, hover, came_from) = (case.seconds_in, case.hover, case.came_from.clone());
            let opened = open_hud_window(cx, move |_| HudView::preview(st, state, seconds, hover, came_from));
            let Some(window) = opened else { continue };
            cx.background_executor().timer(Duration::from_millis(350)).await;
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

/// Follow the real success clock from the processing fill through icon drawing and exit.
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

/// Native waveform recordings against white/dark content, driven through the live normalization filter.
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

/// Capture the real, continuously ticking native renderer; no frozen timestamps or HTML animation.
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

fn held_label_sample(canvas: &RgbaImage) -> Vec<u8> {
    let (w, h) = canvas.dimensions();
    image::imageops::crop_imm(canvas, w / 2 - scaled(18.), h - scaled(39.), scaled(36.), scaled(18.))
        .to_image()
        .into_raw()
}

/// At this row the label cannot contribute: a crisp front must remain visibly stronger than its low-opacity halo
/// in both themes.
fn has_crisp_brush(canvas: &RgbaImage, appearance: Appearance) -> bool {
    let (w, h) = canvas.dimensions();
    (w / 2 - scaled(6.)..w / 2 + scaled(6.)).any(|x| {
        let red = canvas.get_pixel(x, h - scaled(22.))[0];
        if appearance == Appearance::Dark { red > 150 } else { red < 200 }
    })
}

/// Width in design points of the capsule in a HUD window shot (the opaque run of the row through its middle).
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

/// The live HUD window (not a frozen preview) through a hold-mode dictation against the mock pipeline: the capsule
/// pops in narrow, stays narrow while the key is down, and widens when it is released. Prints the capsule's width
/// in design points per frame and saves some of the frames (`hud-live-hold-<n>.png`). Opt-in: `DITO_SHOTS_ONLY=hudlive`.
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

/// Exercise the real tooltip layer on the HUD, including an interrupted transfer and exit.
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

/// The same pointer poll and render paths as the live non-activating panel.
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
            HudView::preview(preview, HudState::Recognizing, 1., PreviewHover::Away, None)
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
            window.update(cx, |view, _, cx| view.preview_pointer(x, y, cx)).expect("pointer poll");
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
