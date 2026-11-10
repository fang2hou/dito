use std::cell::RefCell;
use std::rc::Rc;

use dito_core::config::{Appearance, restore_defaults};
use dito_core::i18n::{Lang, Tr, system_language};
use dito_platform::system::{FramelessWindow, HudNativeWindow};
use gpui_kit::prelude::*;
use gpui_kit::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use super::failure::{Failure, display_path};
use crate::opts::Opts;
use crate::startup;
use crate::state::AppState;
use crate::theme::{self, Palette};
use crate::ui::{self, BtnKind, MotionScope, Root, cbk, text};

/// Where the window leaves the error of a launch that failed with nothing to show, for [`run`](super::run) to return
/// after the app has quit. The slot is shared: `run` keeps a handle to it before handing the global to the app.
#[derive(Default)]
pub(super) struct Ending(pub(super) Rc<RefCell<Option<anyhow::Error>>>);

impl Global for Ending {}

/// What the window needs to carry on with a normal launch once the settings are restored.
#[derive(Clone)]
pub struct Launch {
    pub opts: Opts,
    pub rt: tokio::runtime::Handle,
}

// Metrics of `design:settings/startup-failure`, in points.
const WIDTH: f32 = 620.;
const RADIUS: f32 = 20.;
const BORDER: f32 = 1.;
const PAD_X: f32 = 28.;
const PAD_Y: f32 = 24.;
const GAP: f32 = 16.;
const BUTTON_GAP: f32 = 10.;
const DETAIL_RADIUS: f32 = 8.;
const DETAIL_PAD_X: f32 = 14.;
const DETAIL_PAD_Y: f32 = 12.;
const DETAIL_GAP: f32 = 4.;
const PATH_FONT: &str = "Menlo";

/// Font size and line height of one kind of text.
struct Type {
    size: f32,
    line: f32,
}

const TITLE: Type = Type { size: 20., line: 30. };
const BODY: Type = Type { size: 13., line: 20. };
const DETAIL: Type = Type { size: 12., line: 18. };

/// The card of `design:settings/startup-failure`. It follows the system appearance and language: the saved ones cannot be trusted while the
/// settings cannot be read, and the window must look the same whichever file failed.
pub struct FailureView {
    failure: Failure,
    /// Replaces the reason in the details after a restore that did not work, so the person sees why.
    problem: Option<String>,
    appearance: Appearance,
    launch: Launch,
    motion: MotionScope,
    _appearance: Subscription,
}

impl FailureView {
    fn new(
        failure: Failure,
        appearance: Appearance,
        launch: Launch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            failure,
            problem: None,
            appearance,
            launch,
            motion: MotionScope::new(),
            _appearance: cx.observe_window_appearance(window, |_, _, cx| cx.notify()),
        }
    }

    fn reason(&self, lang: Lang) -> String {
        self.problem.clone().unwrap_or_else(|| self.failure.reason(lang))
    }

    /// Keep the settings file as a backup, write the defaults, then carry on with the launch.
    fn restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match restore_defaults(self.failure.path()) {
            Ok(backup) => {
                tracing::info!(?backup, "the unreadable settings were kept and the defaults written");
                self.carry_on(window, cx);
            }
            Err(error) => {
                tracing::error!("could not restore the default settings: {error}");
                self.problem = Some(error.to_string());
                self.fit(window, cx);
            }
        }
    }

    /// Build the app in this process, now that the settings load, and hand the screen over to it. The data may still
    /// hold another problem (a history from a newer Dito), which gets its own turn in this window.
    fn carry_on(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match AppState::build(self.launch.opts.clone(), &self.launch.rt) {
            Ok(st) => {
                // The windows the launch opens are the app now: closing the last one must not end the process.
                cx.set_quit_mode(QuitMode::Explicit);
                window.remove_window();
                cx.defer(move |cx| startup::launch(st, cx));
            }
            Err(error) => match Failure::from_error(&error, &self.launch.opts) {
                Some(next) => {
                    tracing::warn!("startup stopped again: {error:#}");
                    self.failure = next;
                    self.problem = None;
                    self.fit(window, cx);
                }
                None => {
                    // No window for this one: leave it to `run`, which returns it once GPUI has shut down.
                    *cx.default_global::<Ending>().0.borrow_mut() = Some(error);
                    cx.quit();
                }
            },
        }
    }

    /// Resize the window to the text it now holds, and repaint.
    fn fit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let frame = frameless(window, card_height(&self.failure, &self.reason(system_language())));
        // Named in full: `HasWindowHandle` has a method of the same name.
        let handle = Window::window_handle(window);
        cx.spawn(async move |_, cx| {
            if let Some(frame) = frame {
                frame.apply();
            }
            cx.update(|cx| handle.update(cx, |_, window, _| window.activate_window()).ok());
        })
        .detach();
        cx.notify();
    }

    fn card(&self, p: &Palette, lang: Lang, cx: &mut Context<Self>) -> Div {
        let failure = &self.failure;
        // The card is the window: it fills it exactly (see `frame_window`), so its corner is the only one there is.
        let mut card = div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .gap(px(GAP))
            .px(px(PAD_X))
            .py(px(PAD_Y))
            .rounded(px(RADIUS))
            .border_1()
            .border_color(p.border_strong)
            .bg(p.card)
            // The window has no title bar to drag it by, so the card is the handle: pressing anywhere but on a
            // button or the selectable details (which block this layer) moves it.
            .child(ui::drag_area("failure-drag").absolute().inset_0())
            .child(text(failure.title(lang), TITLE.size, FontWeight::BOLD, p.text).line_height(px(TITLE.line)))
            .child(text(failure.body(lang), BODY.size, FontWeight::NORMAL, p.text).line_height(px(BODY.line)))
            .child(self.details(p, lang));
        if let Some(hint) = failure.hint(lang) {
            card = card.child(text(hint, BODY.size, FontWeight::NORMAL, p.muted).line_height(px(BODY.line)));
        }
        card.child(self.actions(p, lang, cx))
    }

    /// The file and the reason it cannot be used, both selectable: pressing here selects text, never moves the window.
    fn details(&self, p: &Palette, lang: Lang) -> Div {
        let path = ui::selectable_text(
            p,
            "failure-path",
            display_path(self.failure.path()),
            DETAIL.size,
            FontWeight::NORMAL,
            p.text,
        );
        let reason =
            ui::selectable_text(p, "failure-reason", self.reason(lang), DETAIL.size, FontWeight::NORMAL, p.muted);
        div()
            .flex()
            .flex_col()
            .gap(px(DETAIL_GAP))
            .px(px(DETAIL_PAD_X))
            .py(px(DETAIL_PAD_Y))
            .rounded(px(DETAIL_RADIUS))
            .border_1()
            .border_color(p.border)
            .bg(p.sub)
            .occlude()
            .child(path.font_family(PATH_FONT).line_height(px(DETAIL.line)))
            .child(reason.line_height(px(DETAIL.line)))
    }

    /// Finder on the left edge, then Quit and, where it is on offer, Restore at the right.
    fn actions(&self, p: &Palette, lang: Lang, cx: &mut Context<Self>) -> Div {
        let path = self.failure.path().to_owned();
        let finder = ui::button(
            "failure-finder",
            p,
            lang.tr(Tr::MenubarShowInFinder),
            BtnKind::Secondary,
            None,
            Rc::new(move |_, cx| cx.reveal_path(&path)),
        )
        .occlude();
        let quit = |kind| {
            ui::button("failure-quit", p, lang.tr(Tr::StartupQuit), kind, None, Rc::new(|_, cx| cx.quit())).occlude()
        };
        let row = div().flex().items_center().justify_end().gap(px(BUTTON_GAP)).child(finder);
        if !self.failure.can_restore() {
            return row.child(quit(BtnKind::Primary));
        }
        let restore = ui::button(
            "failure-restore",
            p,
            lang.tr(Tr::PromptsRestoreDefaults),
            BtnKind::Primary,
            None,
            cbk(cx, |this: &mut Self, window, cx| this.restore(window, cx)),
        )
        .occlude();
        row.child(div().flex_1()).child(quit(BtnKind::Secondary)).child(restore)
    }
}

impl Render for FailureView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = system_language();
        ui::set_ui_language(lang);
        let p = theme::palette(self.appearance, cx);
        theme::apply(&p, cx);
        let motion = self.motion.handle();
        ui::with_motion_scope(motion, || self.card(&p, lang, cx))
    }
}

/// The height of the window for `failure` with `reason` in its details: the card's border and padding around every
/// block of text, each at the number of lines the renderer wraps it to at the card's width.
fn card_height(failure: &Failure, reason: &str) -> f32 {
    let content = WIDTH - 2. * (BORDER + PAD_X);
    let detail = content - 2. * (BORDER + DETAIL_PAD_X);
    let lines = |s: &str, family: &str, t: &Type, weight: FontWeight, width: f32| {
        ui::text_lines(s, family.to_owned(), t.size, weight, width) as f32 * t.line
    };
    let prose = |s: &str, t: &Type, weight: FontWeight| lines(s, ui::family_for(s), t, weight, content);

    let lang = system_language();
    let path = display_path(failure.path());
    let details = 2. * (BORDER + DETAIL_PAD_Y)
        + lines(&path, PATH_FONT, &DETAIL, FontWeight::NORMAL, detail)
        + DETAIL_GAP
        + lines(reason, ui::family_for(reason), &DETAIL, FontWeight::NORMAL, detail);
    let mut blocks = vec![
        prose(failure.title(lang), &TITLE, FontWeight::BOLD),
        prose(failure.body(lang), &BODY, FontWeight::NORMAL),
        details,
    ];
    blocks.extend(failure.hint(lang).map(|hint| prose(hint, &BODY, FontWeight::NORMAL)));
    blocks.push(ui::CONTROL_H);
    (2. * (BORDER + PAD_Y) + blocks.iter().sum::<f32>() + GAP * (blocks.len() - 1) as f32).ceil()
}

/// The card is the whole window. macOS gives a titled window a frame of its own (a rounded corner mask, a border, a
/// shadow and a background) even when gpui draws no title bar, and that frame would show around the card as a second
/// shape. So the frame is taken away and the window made as large as the card, which then fills it to the corners.
/// `HudNativeWindow` is the app's wrapper for such windows: the HUD is one too. The change is applied later, once the
/// borrow of the app that this runs under is released.
fn frameless(window: &Window, height: f32) -> Option<FramelessWindow> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return None };
    // SAFETY: the view belongs to `window`, which the borrow keeps alive.
    unsafe { HudNativeWindow::from_ns_view(handle.ns_view) }?.frameless((f64::from(WIDTH), f64::from(height)))
}

/// Open the window for `failure`, centered on the screen, as tall as its text. `shown` is false for the screenshot
/// harness, whose windows are never put on screen.
pub fn open(
    failure: Failure,
    appearance: Appearance,
    launch: Launch,
    shown: bool,
    cx: &mut App,
) -> Option<WindowHandle<Root>> {
    let lang = system_language();
    ui::set_ui_language(lang);
    let height = card_height(&failure, &failure.reason(lang));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(WIDTH), px(height)), cx))),
        titlebar: None,
        // The card draws its own rounded corners and border.
        window_background: WindowBackgroundAppearance::Transparent,
        is_resizable: false,
        is_minimizable: false,
        // Not shown until its frame is gone, so the frame never appears.
        focus: false,
        show: false,
        ..Default::default()
    };
    let mut frame = None;
    let window = cx
        .open_window(options, |window, cx| {
            frame = frameless(window, height);
            let view = cx.new(|cx| FailureView::new(failure, appearance, launch, window, cx));
            cx.new(|cx| Root::new(view, window, cx).bg(ui::transparent()))
        })
        .inspect_err(|error| tracing::error!("could not open the startup failure window: {error}"))
        .ok()?;
    cx.spawn(async move |cx| {
        if let Some(frame) = frame {
            frame.apply();
        }
        if shown {
            cx.update(|cx| {
                let _ = window.update(cx, |_, window, _| window.activate_window());
                cx.activate(true);
            });
        }
    })
    .detach();
    Some(window)
}
