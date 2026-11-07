//! The HUD window view: dictation state, hover and the controls above the capsule.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::controller::{Command, HudState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::services;
use crate::settings::{self, Page};
use crate::state::AppState;
use crate::theme;
use crate::ui::{self, Motion};

use super::buttons::{self, Btn, Circle};
use super::capsule;
use super::processing_motion::{Fill, ProcessingMotion};
use super::waveform::Waveform;
use super::{Scale, WIN_PAD, px};

mod accessories;
mod capsule_stack;
mod clock;
mod hit;
mod language_chip;
mod layout;
#[cfg(feature = "screenshots")]
mod probe;
#[cfg(feature = "screenshots")]
pub use probe::PreviewHover;

/// Circle-button hover cross-fade, on the same clock feel as the app's other micro-animations.
const HOVER_MS: u64 = 140;
/// Capsule pop-in: liquid stretch and rise.
pub(crate) const POP_MS: u64 = 420;

/// Hairline of the smaller pills (tooltip, stop, language chip).
const PILL_HAIRLINE: f32 = 0.75;
/// Hovering away keeps the extras for this long, so the pointer can travel to them.
const LEAVE_GRACE: Duration = Duration::from_millis(250);

const LABEL_FADE_S: f32 = 0.12;
type OnPress = Box<dyn Fn(&mut HudView, &mut Window, &mut Context<HudView>)>;

impl Btn {
    /// What pressing the button does in the HUD window.
    fn on_press(self) -> OnPress {
        match self {
            Self::Cancel => Box::new(|v, _, _| v.send(Command::Cancel)),
            Self::Finish => Box::new(|v, _, _| v.send(Command::Finish)),
            Self::Dismiss => Box::new(|v, _, _| v.send(Command::Dismiss)),
            Self::Retry => Box::new(|v, _, _| v.send(Command::Retry)),
            Self::Settings => Box::new(|v, _, cx| {
                v.send(Command::Dismiss);
                let st = v.st.clone();
                settings::open(st, Page::Status, cx);
            }),
        }
    }
}
pub struct HudView {
    pub(super) st: Arc<AppState>,
    state: HudState,
    since: Instant,
    /// Capsule width when the state last changed (design points): the width eases from it to the new
    /// state's own ([`capsule::rest_width`]) over [`WIDEN_S`], so hold mode's narrow capsule widens on release.
    width_from: f32,
    /// Pointer is over the capsule or the controls above it (the whole hover zone).
    hover_zone: bool,
    pointer_in_zone: bool,
    leave: ui::Requests,
    /// Id of the round button under the pointer, if any.
    hover_btn: Option<&'static str>,
    /// When the window opened; drives the pop-in (state changes do not replay it).
    opened: Instant,
    processing: Option<ProcessingMotion>,
    processing_pending: bool,
    displayed_fill: Fill,
    /// Previous processing stage, retained while the brush settles and labels cross-fade.
    previous: Option<HudState>,
    /// Preview mode: fixed animation time, no live updates.
    freeze: Option<f32>,
    /// The HUD's own motion scope: its hover tweens and controls belong to this window's view,
    /// not to any page or the settings shell.
    motion: ui::MotionScope,
    waveform: Waveform,
    wave_tick: Instant,
    frame_clock_started: bool,
    error_reason: super::motion::Spring,
    error_tick: Instant,
    recording_started: Option<Instant>,
    countdown_exit: Option<u64>,
    countdown_tick: Option<u64>,
    #[cfg(feature = "screenshots")]
    wave_preview_gain: Option<f32>,
}

impl HudView {
    pub fn new(st: Arc<AppState>, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
        let mut rx = st.dictation.hud();
        let initial = rx.borrow_and_update().clone();
        let recording_started = st.dictation.recording_started();
        ui::observe(rx, cx, Self::enter_state);
        let mut view = Self { recording_started, ..Self::idle(st, initial) };
        view.start_processing(Fill::default());
        view
    }

    /// A view resting in `state`, with no listeners attached.
    fn idle(st: Arc<AppState>, state: HudState) -> Self {
        Self {
            st,
            width_from: capsule::rest_width(&state),
            state,
            since: Instant::now(),
            hover_zone: false,
            pointer_in_zone: false,
            leave: ui::Requests::default(),
            hover_btn: None,
            opened: Instant::now(),
            processing: None,
            processing_pending: false,
            displayed_fill: Fill::default(),
            previous: None,
            freeze: None,
            motion: ui::MotionScope::new(),
            waveform: Waveform::default(),
            wave_tick: Instant::now(),
            frame_clock_started: false,
            error_reason: super::motion::Spring::default(),
            error_tick: Instant::now(),
            recording_started: None,
            countdown_exit: None,
            countdown_tick: None,
            #[cfg(feature = "screenshots")]
            wave_preview_gain: None,
        }
    }

    fn enter_state(&mut self, state: HudState, cx: &mut Context<Self>) {
        if self.state == state {
            return;
        }
        let elapsed = self.elapsed();
        let displayed = self.displayed_fill;
        self.width_from = self.width();
        let was_listening = matches!(self.state, HudState::Listening { .. });
        let now_listening = matches!(state, HudState::Listening { .. });
        if was_listening && !now_listening {
            self.countdown_exit = self.remaining();
        }
        // A result can arrive during the midpoint chase, while the user still sees Transcribing.
        self.previous = Some(if self.state == HudState::Polishing && elapsed < self.catch_up() + LABEL_FADE_S / 2. {
            HudState::Recognizing
        } else {
            self.state.clone()
        });
        if now_listening && !was_listening {
            self.waveform = Waveform::default();
            self.wave_tick = Instant::now();
            self.recording_started = self.st.dictation.recording_started();
            self.countdown_exit = None;
        }
        if matches!(state, HudState::Error { .. }) {
            self.pointer_in_zone = false;
        }
        self.error_reason = super::motion::Spring::default();
        self.error_tick = Instant::now();
        self.state = state;
        self.since = Instant::now();
        self.start_processing(displayed);
        cx.notify();
    }

    pub(super) fn send(&self, c: Command) {
        self.st.dictation.send(c);
    }

    fn circle_button(
        &self,
        c: Circle,
        on: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let Circle { id, .. } = c;
        let weak = cx.entity().downgrade();
        let w2 = weak.clone();
        // The interactive element stays put; its background and glyph layers cross-fade (see `tweened`).
        let target = if self.hover_btn == Some(id) { 1. } else { 0. };
        let (lc, gc) = (c, c);
        let bg_layer = ui::tweened(ElementId::from((ElementId::from(id), "bg")), target, HOVER_MS, move |t| {
            buttons::hover_layer(&lc, Scale::HUD, t)
        });
        let glyph_layer = ui::tweened(ElementId::from((ElementId::from(id), "glyph")), target, HOVER_MS, move |t| {
            buttons::hover_glyph(&gc, Scale::HUD, t)
        });
        buttons::circle_shell(&c, Scale::HUD, bg_layer, glyph_layer)
            .id(id)
            .cursor_pointer()
            .on_click(move |_, window, app| {
                weak.update(app, |v, cx| on(v, window, cx)).ok();
            })
            .on_hover(move |over: &bool, _, app| {
                w2.update(app, |v, cx| {
                    v.hover_btn = if *over { Some(id) } else { None };
                    cx.notify();
                })
                .ok();
            })
    }

    /// Window-relative hit testing is independent of GPUI hover events: transparent areas must
    /// continue tracking the pointer while the native window lets mouse events pass through.
    pub(super) fn pointer_update(&mut self, x: f32, y: f32, cx: &mut Context<Self>) -> bool {
        let width = self.width();
        let chip_width = language_chip::chip_width(self.st.cfg.get().general.hud_languages());
        let in_zone = hit::pointer_hit(&self.state, width, chip_width, self.hover_zone, x, y).in_zone;
        if in_zone != self.pointer_in_zone {
            self.pointer_in_zone = in_zone;
            if matches!(self.state, HudState::Error { .. }) {
                self.send(Command::ErrorHover(in_zone));
            }
            self.zone_hover(in_zone, cx);
        }
        let pointer = hit::pointer_hit(&self.state, width, chip_width, self.hover_zone, x, y);
        if self.hover_btn != pointer.button {
            self.hover_btn = pointer.button;
            cx.notify();
        }
        pointer.interactive
    }

    pub(super) fn needs_lower_room(&self) -> bool {
        matches!(self.state, HudState::Error { .. })
    }

    /// The pointer entered or left the hover zone (capsule + the controls above it).
    fn zone_hover(&mut self, over: bool, cx: &mut Context<Self>) {
        if over {
            self.leave.bump();
            self.hover_zone = true;
            cx.notify();
            return;
        }
        let req = self.leave.begin();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LEAVE_GRACE).await;
            let _ = this.update(cx, |v, cx| {
                if v.leave.is_current(req) {
                    v.hover_zone = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Entry animation (see [`capsule::pop_shape`]), played once when the window opens; `w` is the capsule's
    /// current width, so a narrow hold-mode capsule pops in narrow. Frozen previews render the settled capsule.
    fn pop_in(&self, cap: Stateful<Div>, w: f32) -> AnyElement {
        if self.freeze.is_some() {
            return cap.into_any_element();
        }
        let motion = Motion::system();
        let e = self.opened.elapsed().as_secs_f32();
        let dur = POP_MS as f32 / 1000.;
        if e >= dur {
            return cap.into_any_element();
        }
        cap.with_animation(
            (ElementId::from("hud-capsule"), "pop"),
            Animation::new(Duration::from_millis(POP_MS)),
            move |c, t| capsule::pop_shape(c, Scale::HUD, w, t, motion),
        )
        .into_any_element()
    }
}

impl Render for HudView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.freeze.is_none() && !self.frame_clock_started {
            self.frame_clock_started = true;
            Self::next_frame(window, cx.entity().downgrade());
        }
        // The HUD window owns its transient motion state; without this its hover tweens would run
        // in whatever ambient scope happened to be left over.
        let scope = self.motion.handle();
        ui::with_motion_scope(scope, || ui::tooltip_root(self.render_scoped(cx)))
    }
}

impl HudView {
    /// Follow this window's display link. A quiet ocean still skips redundant paints.
    fn next_frame(window: &Window, view: WeakEntity<Self>) {
        window.on_next_frame(move |window, app| {
            let alive = view.update(app, |view, cx| {
                if view.advance_clock() {
                    cx.notify();
                }
            });
            if alive.is_ok() {
                Self::next_frame(window, view);
            }
        });
    }

    fn render_scoped(&mut self, cx: &mut Context<Self>) -> Div {
        let first_processing_frame = std::mem::take(&mut self.processing_pending);
        if first_processing_frame {
            // Start on the first rendered frame, even if backend notifications arrived before it.
            self.since = Instant::now();
        }
        let elapsed = if first_processing_frame { 0. } else { self.elapsed() };
        self.displayed_fill = self.processing.map(|motion| motion.at(elapsed)).unwrap_or_default();
        let cfg = self.st.cfg.get();
        let lang = cfg.general.lang();
        let p = theme::palette(cfg.general.hud_appearance, cx);
        ui::set_ui_language(lang);
        let error_detail =
            if let HudState::Error { kind, .. } = &self.state { Some(ui::error_summary(lang, *kind)) } else { None };
        let above = self.tips_above(lang, &p, error_detail);
        let capsule = self.pop_in(self.capsule(p, cx), self.width());
        let zone = self.zone(&p, lang, capsule, cx);
        let countdown = self
            .remaining()
            .map(|s| (s, true))
            .or_else(|| (self.elapsed() < LABEL_FADE_S).then_some(self.countdown_exit).flatten().map(|s| (s, false)));
        let mut root = div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .justify_end()
            .items_center()
            .pb(px(layout::BOTTOM_INSET + WIN_PAD))
            .pl(px(WIN_PAD))
            .pr(px(WIN_PAD))
            .gap(px(layout::ZONE_GAP))
            .font_family(ui::cjk_family());
        if let Some(msg) = error_detail {
            root = root.child(self.error_reason_bubble(msg, &p));
        }
        root = root.child(above).child(zone);
        if let Some((seconds, active)) = countdown {
            root = root.child(self.countdown(&p, seconds, active));
        }
        root
    }
}
