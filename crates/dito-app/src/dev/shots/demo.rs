use std::cell::Cell;
use std::path::Path;
use std::time::{Duration, Instant};

use dito_core::config::RecordMode;
use dito_core::i18n::Lang;
use gpui_kit::*;

use super::capture::{close_window, render, save};
use crate::onboarding::demo::{self, Frozen};
use crate::theme::Palette;
use crate::ui;

thread_local! {
    /// How many times the demo window's view rendered (one per animation frame while the loop runs).
    static RENDERS: Cell<usize> = const { Cell::new(0) };
}

/// A bare window with the wizard's usage demo on a card-coloured ground.
struct DemoShot {
    p: Palette,
    lang: Lang,
    mode: RecordMode,
    key: SharedString,
}

impl DemoShot {
    fn new(p: Palette, lang: Lang, mode: RecordMode) -> Self {
        let key = dito_platform::Shortcut::parse("RightOption")
            .map(|s| dito_platform::keymap::label(&s, lang))
            .unwrap_or_default();
        Self { p, lang, mode, key: key.into() }
    }
}

impl Render for DemoShot {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        ui::set_ui_language(self.lang);
        RENDERS.with(|r| r.set(r.get() + 1));
        div().size_full().bg(self.p.card).p(px(24.)).font_family(ui::cjk_family()).child(demo::usage_demo(
            &self.p,
            self.lang,
            self.mode,
            self.key.clone(),
        ))
    }
}

fn open_demo_window(cx: &mut AsyncApp, view: DemoShot) -> Option<WindowHandle<DemoShot>> {
    cx.update(|cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(624.), px(234.)), cx))),
            titlebar: None,
            ..Default::default()
        };
        cx.open_window(options, |_, cx| cx.new(|_| view)).ok()
    })
}

/// The usage demo on its own clock, not frozen: renders for a few seconds, switching the mode in between, and prints
/// what each frame shows (`demo-live-<n>.png` for some). Opt-in: `DITO_SHOTS_ONLY=demolive`.
pub async fn demo_live_shots(dir: &Path, cx: &mut AsyncApp) {
    let Some(handle) = open_demo_window(cx, DemoShot::new(Palette::light(), Lang::Ja, RecordMode::Toggle)) else {
        return;
    };
    let start = Instant::now();
    let mut switched = false;
    let mut n = 0;
    while start.elapsed() < Duration::from_millis(7500) {
        if !switched && start.elapsed() >= Duration::from_millis(3000) {
            switched = true;
            let _ = cx.update(|cx| {
                handle.update(cx, |view, _, cx| {
                    view.mode = RecordMode::Hold;
                    cx.notify();
                })
            });
        }
        let (active, img) = cx.update(|cx| {
            handle
                .update(cx, |_, w, _| (w.is_window_active(), w.render_to_image().map_err(|e| e.to_string())))
                .ok()
                .unzip()
        });
        println!(
            "demo-live frame {n:2}: +{:4} ms, window active {active:?}, renders so far {}",
            start.elapsed().as_millis(),
            RENDERS.with(|r| r.get())
        );
        if let Some(Ok(img)) = img {
            save(dir, &format!("demo-live-{n:02}.png"), img);
        }
        n += 1;
        // Closer frames around the mode switch, where the cross-fade happens.
        let gap = if (3000..3400).contains(&start.elapsed().as_millis()) { 50 } else { 250 };
        cx.background_executor().timer(Duration::from_millis(gap)).await;
    }
    close_window(cx, handle);
}

struct DemoJob {
    mode: RecordMode,
    mode_name: &'static str,
    dark: bool,
    lang: Lang,
    frozen: Frozen,
    suffix: String,
}

impl DemoJob {
    fn window_key(&self) -> (RecordMode, bool, Lang) {
        (self.mode, self.dark, self.lang)
    }

    fn file_name(&self) -> String {
        let theme = if self.dark { "dark" } else { "light" };
        let lang = if self.lang == Lang::Ja { String::new() } else { format!("-{}", self.lang.code()) };
        format!("demo-{}-{theme}{lang}-{}.png", self.mode_name, self.suffix)
    }
}

fn demo_jobs() -> Vec<DemoJob> {
    let toggle = [0.0, 0.66, 0.83, 1.4, 2.0, 3.04, 3.2, 4.0, 4.7, 4.88, 5.3, 5.7];
    let hold = [0.0, 0.66, 0.83, 1.4, 2.0, 3.04, 3.15, 3.2, 3.6, 4.2, 4.88, 5.3, 5.7];
    let mut jobs = Vec::new();
    for (mode, mode_name, times) in [(RecordMode::Toggle, "toggle", &toggle[..]), (RecordMode::Hold, "hold", &hold[..])]
    {
        for dark in [false, true] {
            let job = |lang, frozen, suffix: String| DemoJob { mode, mode_name, dark, lang, frozen, suffix };
            jobs.push(job(Lang::Ja, Frozen::Still, "still".to_string()));
            jobs.extend(
                times.iter().map(|&at| job(Lang::Ja, Frozen::At(at), format!("t{:04}", (at * 1000.).round() as u32))),
            );
            for lang in [Lang::En, Lang::Zh] {
                jobs.push(job(lang, Frozen::Still, "still".to_string()));
                jobs.push(job(lang, Frozen::At(4.0), "t4000".to_string()));
            }
        }
    }
    jobs
}

/// The wizard's usage demo held at several moments of its loop, both modes, light and dark
/// (`demo-<mode>-<theme>-t<ms>.png`), the Reduce Motion still (`…-still.png`, the design's static frame), and the
/// still and the recognizing capsule in English and Chinese (`demo-<mode>-<theme>-<lang>-…`).
pub async fn demo_shots(dir: &Path, cx: &mut AsyncApp) {
    let mut open: Option<((RecordMode, bool, Lang), WindowHandle<DemoShot>)> = None;
    for job in demo_jobs() {
        let key = job.window_key();
        if open.as_ref().is_none_or(|(have, _)| *have != key) {
            if let Some((_, window)) = open.take() {
                close_window(cx, window);
            }
            let palette = if job.dark { Palette::dark() } else { Palette::light() };
            let Some(window) = open_demo_window(cx, DemoShot::new(palette, job.lang, job.mode)) else { continue };
            open = Some((key, window));
        }
        let Some((_, window)) = &open else { continue };
        demo::freeze(Some(job.frozen));
        let _ = cx.update(|cx| window.update(cx, |_, w, _| w.refresh()));
        cx.background_executor().timer(Duration::from_millis(150)).await;
        if let Some(img) = render(cx, *window) {
            save(dir, &job.file_name(), img);
        }
    }
    if let Some((_, window)) = open.take() {
        close_window(cx, window);
    }
    demo::freeze(None);
}
