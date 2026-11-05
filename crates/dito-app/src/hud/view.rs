//! The HUD window view: dictation state, hover and the controls above the capsule.

use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::config::{PreferredLanguage, RecordMode};
use dito_core::controller::{Command, ErrorAction, HudState, MAX_RECORDING_DURATION};
use dito_core::i18n::{Lang, Tr};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::HudIc;
use crate::settings::{self, Page};
use crate::state::AppState;
use crate::theme::{PLAIN_BLACK, Palette, hex};
use crate::ui::{hex_alpha, hud_icon, text};

#[cfg(feature = "screenshots")]
use super::drawing::NARROW_W;
use super::drawing::{
    BTN, Btn, CAPSULE_PAD, CAPSULE_W, COPY_END, COPY_FADE_S, COPY_HOLD_END, Capsule, Circle, OUTCOME_END,
    OUTCOME_FADE_S, OUTCOME_HOLD_END, OUTCOME_ICON_DELAY, SETTLE_S, WIDEN_S, Wave, circle_shell, copied_growth,
    eased_width, hud_color, motion_fill_at, pop_shape, rest_width, settle, surface_shadow,
};
use super::waveform::Waveform;
use super::{Scale, WIN_PAD, WIN_W, px, tx};

/// Circle-button hover cross-fade, on the same clock feel as the app's other micro-animations.
const HOVER_MS: u64 = 140;
/// Capsule pop-in: liquid stretch and rise.
pub(crate) const POP_MS: u64 = 420;

/// Hairline of the smaller pills (tooltip, stop, language chip).
const PILL_HAIRLINE: f32 = 0.75;
/// Hovering away keeps the extras for this long, so the pointer can travel to them.
const LEAVE_GRACE: Duration = Duration::from_millis(250);

const LABEL_FADE_S: f32 = 0.12;
const SUCCESS_FADE_S: f32 = 0.20;
const ERROR_BUBBLE_H: f32 = 30.5;
const CAPSULE_TOP: f32 = super::window::WIN_H + 2. * WIN_PAD - (35. + WIN_PAD) - 33.;

/// Returns (hover zone, hovered recording button, mouse-interactive region).
fn pointer_hit(state: &HudState, width: f32, hover_zone: bool, x: f32, y: f32) -> (bool, Option<&'static str>, bool) {
    let center = (WIN_W + 2. * WIN_PAD) / 2.;
    let top = CAPSULE_TOP;
    let left = center - width / 2.;
    let on_capsule = x >= left && x <= left + width && y >= top && y <= top + 33.;
    let toggle = matches!(state, HudState::Listening { mode: RecordMode::Toggle });
    let error = matches!(state, HudState::Error { .. });
    let processing = matches!(state, HudState::Recognizing | HudState::Polishing);
    let inside_circle = |cx: f32| (x - cx).powi(2) + (y - (top + 16.5)).powi(2) <= (BTN / 2.).powi(2);
    let left_button = (toggle || error) && inside_circle(center - CAPSULE_W / 2. + CAPSULE_PAD + BTN / 2.);
    let right_button = (toggle || error) && inside_circle(center + CAPSULE_W / 2. - CAPSULE_PAD - BTN / 2.);
    let in_upper = x >= center - 90. && x <= center + 90. && y >= top - 38. && y < top;
    let in_error_note = error
        && x >= center - 135.
        && x <= center + 135.
        && y >= top - ERROR_BUBBLE_H - 6.
        && y <= top + 33. + 6. + ERROR_BUBBLE_H + 4.;
    let in_zone = on_capsule || in_error_note || ((toggle || processing) && hover_zone && in_upper);
    let hovered = if toggle && left_button {
        Some("hud-cancel")
    } else if toggle && right_button {
        Some("hud-finish")
    } else if error && left_button {
        Some("hud-dismiss")
    } else if error && right_button {
        match state {
            HudState::Error { action: ErrorAction::Retry, .. } => Some("hud-retry"),
            HudState::Error { action: ErrorAction::OpenSettings, .. } => Some("hud-settings"),
            _ => None,
        }
    } else {
        None
    };
    let chip = toggle
        && hover_zone
        && hovered.is_none()
        && x >= center - 85.
        && x <= center + 85.
        && y >= top - 32.
        && y <= top - 6.;
    let stop = processing && hover_zone && x >= center - 31. && x <= center + 31. && y >= top - 35. && y <= top - 6.;
    (in_zone, hovered, left_button || right_button || chip || stop)
}

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
    /// state's own ([`rest_width`]) over [`WIDEN_S`], so hold mode's narrow capsule widens on release.
    width_from: f32,
    /// Pointer is over the capsule or the controls above it (the whole hover zone).
    hover_zone: bool,
    pointer_in_zone: bool,
    leave_gen: u64,
    /// Id of the round button under the pointer, if any.
    hover_btn: Option<&'static str>,
    /// When the window opened; drives the pop-in (state changes do not replay it).
    opened: Instant,
    /// Fill the previous state was showing when the state changed: the next phase eases from it,
    /// so a model that finishes early lands the remaining fill fast instead of jumping.
    carry: Option<f32>,
    /// Previous processing stage, retained while the brush settles and labels cross-fade.
    previous: Option<HudState>,
    /// Preview mode: fixed animation time, no live updates.
    freeze: Option<f32>,
    /// The HUD's own motion scope: its hover tweens and controls belong to this window's view,
    /// not to any page or the settings shell.
    motion: crate::ui::MotionScope,
    waveform: Waveform,
    wave_tick: Instant,
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
        settings::watch_config(&st, cx);
        let mut rx = st.dictation.hud();
        let initial = rx.borrow_and_update().clone();
        let recording_started = st.dictation.recording_started();
        cx.spawn(async move |this, cx| {
            while rx.changed().await.is_ok() {
                let s = rx.borrow_and_update().clone();
                if this
                    .update(cx, |t, cx| {
                        let carried = t.progress();
                        t.width_from = t.width();
                        if matches!(t.state, HudState::Listening { .. }) && !matches!(s, HudState::Listening { .. }) {
                            t.countdown_exit = t.remaining();
                        }
                        t.previous = Some(t.state.clone());
                        if matches!(s, HudState::Listening { .. }) && !matches!(t.state, HudState::Listening { .. }) {
                            t.waveform = Waveform::default();
                            t.wave_tick = Instant::now();
                            t.recording_started = t.st.dictation.recording_started();
                            t.countdown_exit = None;
                        }
                        if matches!(s, HudState::Error { .. }) {
                            t.pointer_in_zone = false;
                        }
                        t.error_reason = super::motion::Spring::default();
                        t.error_tick = Instant::now();
                        t.state = s;
                        t.since = Instant::now();
                        t.carry = carried;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        // Animation clock while something is moving. Ticks at display rate; the plain dirty
        // renders it causes are not subject to the platform's inactive-window frame cap (only
        // forced/callback frames are), and the NapGuard held while the HUD is open keeps macOS
        // from throttling this background app's frame commits at all.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(16)).await;
                let alive = this.update(cx, |t, cx| {
                    if matches!(t.state, HudState::Listening { .. }) {
                        let now = Instant::now();
                        let dt = now.duration_since(t.wave_tick).as_secs_f32();
                        t.wave_tick = now;
                        let level = t.st.recorder.level();
                        #[cfg(feature = "screenshots")]
                        let level = t.wave_preview_gain.map_or(level, |gain| {
                            super::waveform::speech_sample(t.opened.elapsed().as_secs_f32(), gain)
                        });
                        t.waveform.advance(level, dt);
                    }
                    let error_target = f32::from(t.hover_btn.is_some());
                    let error_moving = matches!(t.state, HudState::Error { .. }) && t.error_reason.moving(error_target);
                    let now = Instant::now();
                    let dt = now.duration_since(t.error_tick).as_secs_f32();
                    t.error_tick = now;
                    if matches!(t.state, HudState::Error { .. }) {
                        if dito_platform::system::reduce_motion() {
                            t.error_reason = super::motion::Spring { position: error_target, velocity: 0. };
                        } else {
                            t.error_reason.advance(error_target, dt, 0.72, 22.);
                        }
                    }
                    let moving =
                        matches!(t.state, HudState::Listening { .. } | HudState::Recognizing | HudState::Polishing);
                    let reduced = dito_platform::system::reduce_motion();
                    let entry = t.opened.elapsed().as_millis() < POP_MS as u128;
                    let countdown = t.remaining();
                    let countdown_changed = countdown != t.countdown_tick;
                    t.countdown_tick = countdown;
                    let countdown_entering = countdown.is_some()
                        && t.recording_started.is_some_and(|s| (240.0..240.18).contains(&s.elapsed().as_secs_f32()));
                    if (moving && (!reduced || t.elapsed() < 0.30))
                        || entry
                        || t.widening()
                        || t.completing()
                        || error_moving
                        || countdown_changed
                        || countdown_entering
                        || (t.countdown_exit.is_some() && t.elapsed() < LABEL_FADE_S)
                    {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        Self {
            st,
            width_from: rest_width(&initial),
            state: initial,
            since: Instant::now(),
            hover_zone: false,
            pointer_in_zone: false,
            leave_gen: 0,
            hover_btn: None,
            opened: Instant::now(),
            carry: None,
            previous: None,
            freeze: None,
            motion: crate::ui::MotionScope::new(),
            waveform: Waveform::default(),
            wave_tick: Instant::now(),
            error_reason: super::motion::Spring::default(),
            error_tick: Instant::now(),
            recording_started,
            countdown_exit: None,
            countdown_tick: None,
            #[cfg(feature = "screenshots")]
            wave_preview_gain: None,
        }
    }

    /// A frozen HUD for screenshots: `t` seconds into `state`, optionally hovering. `from_hold`: the state
    /// before was hold-mode listening, so the capsule is still widening from its narrow width.
    #[cfg(feature = "screenshots")]
    pub fn preview(
        st: Arc<AppState>,
        state: HudState,
        t: f32,
        hover_zone: bool,
        hover_btn: u8,
        from_hold: bool,
    ) -> Self {
        let hover_btn = match (&state, hover_btn) {
            (HudState::Error { .. }, 1) => Some("hud-dismiss"),
            (HudState::Error { action: ErrorAction::Retry, .. }, 2) => Some("hud-retry"),
            (HudState::Error { .. }, 2) => Some("hud-settings"),
            (_, 1) => Some("hud-cancel"),
            (_, 2) => Some("hud-finish"),
            _ => None,
        };
        Self {
            st,
            width_from: if from_hold { NARROW_W } else { rest_width(&state) },
            state,
            since: Instant::now(),
            hover_zone,
            pointer_in_zone: hover_zone,
            leave_gen: 0,
            hover_btn,
            opened: Instant::now(),
            carry: None,
            previous: None,
            freeze: Some(t),
            motion: crate::ui::MotionScope::new(),
            waveform: Waveform::default(),
            wave_tick: Instant::now(),
            error_reason: super::motion::Spring { position: f32::from(hover_btn.is_some()), velocity: 0. },
            error_tick: Instant::now(),
            recording_started: None,
            countdown_exit: None,
            countdown_tick: None,
            wave_preview_gain: None,
        }
    }

    /// The real renderer and animation clock, held in a processing stage for motion capture.
    #[cfg(feature = "screenshots")]
    pub fn motion_preview(st: Arc<AppState>, cx: &mut Context<Self>) -> Self {
        let mut view = Self::new(st, cx);
        view.state = HudState::Recognizing;
        view.width_from = CAPSULE_W;
        // Keep the brush and glyph sweep visible without replaying the entry spring.
        view.opened -= Duration::from_millis(POP_MS);
        view
    }

    #[cfg(feature = "screenshots")]
    pub fn outcome_preview(st: Arc<AppState>, copy: bool, cx: &mut Context<Self>) -> Self {
        let mut view = Self::new(st, cx);
        view.state = if copy { HudState::Copied } else { HudState::Inserted };
        view.previous = Some(HudState::Recognizing);
        view.carry = Some(0.485);
        view.width_from = CAPSULE_W;
        view.opened -= Duration::from_millis(POP_MS);
        view
    }

    /// Same live filter/renderer with a repeatable input level for visual verification.
    #[cfg(feature = "screenshots")]
    pub fn waveform_preview(st: Arc<AppState>, gain: f32, cx: &mut Context<Self>) -> Self {
        let mut view = Self::new(st, cx);
        view.state = HudState::Listening { mode: RecordMode::Toggle };
        view.width_from = CAPSULE_W;
        view.hover_zone = true;
        view.wave_preview_gain = Some(gain);
        view.opened -= Duration::from_millis(POP_MS);
        view
    }

    fn elapsed(&self) -> f32 {
        self.freeze.unwrap_or_else(|| self.since.elapsed().as_secs_f32())
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
        let bg_layer = crate::ui::tweened(ElementId::from((ElementId::from(id), "bg")), target, HOVER_MS, move |t| {
            super::drawing::hover_layer(&lc, Scale::HUD, t)
        });
        let glyph_layer =
            crate::ui::tweened(ElementId::from((ElementId::from(id), "glyph")), target, HOVER_MS, move |t| {
                super::drawing::hover_glyph(&gc, Scale::HUD, t)
            });
        circle_shell(&c, Scale::HUD, bg_layer, glyph_layer)
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

    /// Capsule width in design points: eases from where it was when the state changed to the state's own.
    /// Copied uses one continuous spring toward the full localized label width.
    fn width(&self) -> f32 {
        let reduced = dito_platform::system::reduce_motion();
        let mut w = if reduced {
            rest_width(&self.state)
        } else {
            eased_width(self.width_from, rest_width(&self.state), self.elapsed())
        };
        if matches!(self.state, HudState::Copied) {
            let t = (self.elapsed() - OUTCOME_ICON_DELAY).max(0.);
            let clock = self.freeze.map_or(Some(t), |f| (f > 0.).then_some(f));
            w += copied_growth(clock, reduced, self.st.cfg.get().general.lang());
        }
        w
    }

    /// Window-relative hit testing is independent of GPUI hover events: transparent areas must
    /// continue tracking the pointer while the native window lets mouse events pass through.
    pub(super) fn pointer_update(&mut self, x: f32, y: f32, cx: &mut Context<Self>) -> bool {
        let (in_zone, _, _) = pointer_hit(&self.state, self.width(), self.hover_zone, x, y);
        if in_zone != self.pointer_in_zone {
            self.pointer_in_zone = in_zone;
            if matches!(self.state, HudState::Error { .. }) {
                self.send(Command::ErrorHover(in_zone));
            }
            self.zone_hover(in_zone, cx);
        }
        let (_, hovered, interactive) = pointer_hit(&self.state, self.width(), self.hover_zone, x, y);
        if self.hover_btn != hovered {
            self.hover_btn = hovered;
            cx.notify();
        }
        interactive
    }

    pub(super) fn needs_lower_room(&self) -> bool {
        matches!(self.state, HudState::Error { .. })
    }

    fn remaining(&self) -> Option<u64> {
        if !matches!(self.state, HudState::Listening { .. }) {
            return None;
        }
        let elapsed = self.freeze.unwrap_or_else(|| self.recording_started.map_or(0., |t| t.elapsed().as_secs_f32()));
        (elapsed >= 240.).then(|| ((MAX_RECORDING_DURATION.as_secs_f32() - elapsed).max(0.) as f64).ceil() as u64)
    }

    /// The capsule is still easing toward its width.
    fn widening(&self) -> bool {
        self.freeze.is_none() && self.since.elapsed().as_secs_f32() < WIDEN_S
    }

    fn completing(&self) -> bool {
        let end = match self.state {
            HudState::Copied => COPY_END,
            HudState::Inserted => OUTCOME_END,
            _ => return false,
        };
        self.freeze.is_none() && self.elapsed() < end
    }

    fn progress(&self) -> Option<f32> {
        if matches!(self.state, HudState::Inserted | HudState::Copied) {
            return self.carry.map(|from| settle(from, 1., self.elapsed()));
        }
        motion_fill_at(&self.state, self.elapsed(), self.carry, dito_platform::system::reduce_motion())
    }

    fn capsule(&self, p: Palette, cx: &mut Context<Self>) -> Stateful<Div> {
        let elapsed = self.elapsed();
        let reduced = dito_platform::system::reduce_motion();
        let clock = self.freeze.unwrap_or_else(|| self.opened.elapsed().as_secs_f32());
        let build = |state, fill| Capsule {
            state,
            lang: self.st.cfg.get().general.lang(),
            scale: Scale::HUD,
            palette: p,
            reduced,
            w: self.width(),
            t: clock,
            wave: if reduced || self.freeze.is_some() { Wave::Rest } else { Wave::Bars(self.waveform.heights()) },
            fill,
            outcome_t: self
                .freeze
                .map_or_else(|| Some((elapsed - OUTCOME_ICON_DELAY).max(0.)), |t| (t > 0.).then_some(t)),
        };
        let mut draw = |state, fill| {
            build(state, fill).element(&mut |b| self.circle_button(b.look(&p), b.on_press(), cx).into_any_element())
        };
        let processing_before =
            self.previous.as_ref().filter(|s| matches!(s, HudState::Recognizing | HudState::Polishing));
        let terminal = matches!(self.state, HudState::Inserted | HudState::Copied);
        let changing_phase = self.state == HudState::Polishing && self.carry.is_some();
        let transition = if terminal { SUCCESS_FADE_S } else { LABEL_FADE_S };
        let mut cap = if self.state == HudState::Recognizing
            && self.previous.as_ref().is_some_and(|s| matches!(s, HudState::Listening { .. }))
            && elapsed < LABEL_FADE_S
        {
            let old = build(self.previous.clone().unwrap(), None)
                .element(&mut |b| super::drawing::static_button(b, Scale::HUD, &p).into_any_element());
            let next = draw(self.state.clone(), self.progress());
            div()
                .relative()
                .child(old)
                .child(next.absolute().top_0().left_0().opacity((elapsed / LABEL_FADE_S).clamp(0., 1.)))
        } else if let Some(previous) = processing_before
            && (terminal || changing_phase)
            && elapsed < SETTLE_S + transition
        {
            let blend = ((elapsed - SETTLE_S) / transition).clamp(0., 1.);
            let old = draw(previous.clone(), self.progress());
            let next = draw(self.state.clone(), if terminal { None } else { self.progress() });
            div().relative().child(old).child(next.absolute().top_0().left_0().opacity(blend))
        } else {
            draw(self.state.clone(), self.progress().filter(|_| !terminal))
        };
        if terminal && self.freeze.is_none() {
            let (hold_end, fade) = if matches!(self.state, HudState::Copied) {
                (COPY_HOLD_END, COPY_FADE_S)
            } else {
                (OUTCOME_HOLD_END, OUTCOME_FADE_S)
            };
            cap = cap.opacity((1. - (elapsed - hold_end) / fade).clamp(0., 1.));
        }
        cap.id("hud-capsule")
    }

    fn bubble(s: &str, p: &Palette) -> Div {
        // Design ratio: 0.89 em side / 0.44 em vertical padding around the font's natural line
        // box (M PLUS 1p auto ≈ 1.48 em) — the bubble's air comes from the leading. At the 12 pt
        // run that is 10.5 / 5.5 padding and a radius of 8, ~30 pt tall overall.
        div()
            .px(px(10.5))
            .py(px(5.5))
            .rounded(px(8.))
            .bg(p.hud_bg)
            .border(px(PILL_HAIRLINE))
            .border_color(p.hud_border)
            .shadow(surface_shadow(p, Scale::HUD, false))
            .flex_none()
            .whitespace_nowrap()
            .child(text(s.to_string(), tx(12.), FontWeight::NORMAL, p.text).line_height(px(18.)))
    }

    /// The pointer entered or left the hover zone (capsule + the controls above it).
    fn zone_hover(&mut self, over: bool, cx: &mut Context<Self>) {
        if over {
            self.leave_gen += 1;
            self.hover_zone = true;
            cx.notify();
            return;
        }
        self.leave_gen += 1;
        let gen_id = self.leave_gen;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LEAVE_GRACE).await;
            let _ = this.update(cx, |v, cx| {
                if v.leave_gen == gen_id {
                    v.hover_zone = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn pill(&self, id: &'static str, p: &Palette) -> Stateful<Div> {
        div()
            .id(id)
            .px(px(10.))
            .py(px(5.))
            .rounded_full()
            .bg(hud_color(p, "#FFEFEF", "#62423C"))
            .border(px(PILL_HAIRLINE))
            .border_color(hex("#FF494966"))
            .shadow(surface_shadow(p, Scale::HUD, false))
            .flex()
            .items_center()
            .gap(px(5.))
            .cursor_pointer()
    }

    /// Language selector above the capsule: the same animated segmented control as the settings,
    /// sized for the HUD (dark track, gold sliding plate). Returns a builder that draws it at scale
    /// `k`, so the hover pop can grow it like a tooltip (HUD 21.07.2).
    fn language_chip(&self, palette: &Palette, cx: &mut Context<Self>) -> impl Fn(f32) -> Div + 'static {
        let cfg = self.st.cfg.get();
        let current = match cfg.general.preferred_language {
            PreferredLanguage::Ja => Lang::Ja,
            PreferredLanguage::En => Lang::En,
            PreferredLanguage::Zh => Lang::Zh,
            PreferredLanguage::Auto => cfg.general.lang(),
        };
        let selected = match current {
            Lang::Ja => 0,
            Lang::En => 1,
            Lang::Zh => 2,
        };
        let mut p = *palette;
        p.seg = p.hud_bg;
        p.seg_on = hud_color(&p, "#FFECBD", "#E0C47D");
        p.text = hex(PLAIN_BLACK);
        p.muted = hud_color(&p, "#6A6455", "#D4D1C9");
        let weak = cx.entity().downgrade();
        let palette = *palette;
        move |k| {
            let weak = weak.clone();
            // The plate springs whenever its target moves, and scaling moves it every frame; popping
            // frames draw under their own (quantized) ids so each one renders settled at its size.
            let id: SharedString =
                if k >= 0.999 { "hud-lang".into() } else { format!("hud-lang-pop-{}", (k * 100.).round()).into() };
            let seg = crate::ui::seg_with(
                id,
                &p,
                &[SharedString::from("日本語"), SharedString::from("English"), SharedString::from("中文")],
                selected,
                Rc::new(move |i, _, app| {
                    weak.update(app, |v, cx| {
                        let pref = match i {
                            0 => PreferredLanguage::Ja,
                            1 => PreferredLanguage::En,
                            _ => PreferredLanguage::Zh,
                        };
                        let _ = v.st.cfg.update(|c| c.general.preferred_language = pref);
                        cx.notify();
                    })
                    .ok();
                }),
                crate::ui::SegStyle {
                    h: tx(26.) * k,
                    label: tx(12.) * k,
                    cell_pad: tx(10.) * k,
                    inset: tx(3.) * k,
                    gap: tx(2.) * k,
                    hover: Some((hex_alpha(palette.text, 0.12), palette.text)),
                },
            );
            div()
                .rounded_full()
                .border(px(PILL_HAIRLINE))
                .bg(palette.hud_bg)
                .border_color(palette.hud_border)
                .shadow(surface_shadow(&palette, Scale::HUD, false))
                .child(seg)
        }
    }

    /// A tooltip centred above the capsule button whose centre is `center_x` from the window's
    /// left edge, in the full-window frame ([0, WIN_W + 2·WIN_PAD]). The anchor is a box placed
    /// symmetrically about that centre (`left = center_x - r`, width `2r`), so `justify_center`
    /// centres the bubble on the button without measuring the text (measured widths drift from
    /// the rendered run, which broke centring whenever the language changed).
    ///
    /// Both frame facts are measured on screen, not assumed: an absolute child's `left` counts
    /// from the window's edge, and the capsule — like every centred child of the root — centres
    /// on the full window width, because the root's horizontal padding does not shift that
    /// centring (its bottom padding does apply).
    ///
    /// The anchor box itself is a plain static element that stays mounted (an animated one
    /// re-lays out against the wrong ancestor); the pop lives in its inner child — the shared
    /// motion/tip pop (design 12.01), growing out of the button while rising and fading in, 150 ms ease-out.
    fn button_tip(&self, id: &'static str, center_x: f32, s: &str, on: bool, p: &Palette) -> Div {
        let full = WIN_W + 2. * WIN_PAD;
        let r = center_x.min(full - center_x);
        // Both insets stay deliberately non-zero (a zero-valued `left` measures as if unset),
        // and the box stretches between them instead of taking an explicit width.
        div()
            .absolute()
            .bottom(px(0.))
            .left(px((center_x - r + 0.5).max(0.5)))
            .right(px((full - center_x - r - 0.5).max(0.5)))
            .flex()
            .justify_center()
            .child(if matches!(self.state, HudState::Error { .. }) {
                let label = s.to_string();
                let palette = *p;
                if self.freeze.is_some() {
                    Self::bubble(&label, &palette).opacity(if on { 1. } else { 0. }).into_any_element()
                } else {
                    crate::ui::tweened(
                        ElementId::from((ElementId::from(id), "tip")),
                        if on { 1. } else { 0. },
                        if on { 120 } else { 100 },
                        move |v| Self::bubble(&label, &palette).opacity(v),
                    )
                }
            } else {
                crate::ui::tip_pop_with(
                    p,
                    ElementId::from((ElementId::from(id), "tip")),
                    SharedString::from(s),
                    on,
                    p.hud_bg,
                    p.text,
                )
            })
    }

    /// Entry animation (see [`pop_shape`]), played once when the window opens; `w` is the capsule's
    /// current width, so a narrow hold-mode capsule pops in narrow. Frozen previews render the settled capsule.
    fn pop_in(&self, cap: Stateful<Div>, w: f32) -> AnyElement {
        if self.freeze.is_some() {
            return cap.into_any_element();
        }
        let reduced = dito_platform::system::reduce_motion();
        let e = self.opened.elapsed().as_secs_f32();
        let dur = POP_MS as f32 / 1000.;
        if e >= dur {
            return cap.into_any_element();
        }
        cap.with_animation(
            (ElementId::from("hud-capsule"), "pop"),
            Animation::new(Duration::from_millis(POP_MS)),
            move |c, t| pop_shape(c, Scale::HUD, w, t, reduced),
        )
        .into_any_element()
    }
}

impl Render for HudView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The HUD window owns its transient motion state; without this its hover tweens would run
        // in whatever ambient scope happened to be left over.
        let scope = self.motion.handle();
        crate::ui::with_motion_scope(scope, || self.render_scoped(_window, cx))
    }
}

impl HudView {
    fn render_scoped(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cfg = self.st.cfg.get();
        let lang = cfg.general.lang();
        let p = crate::theme::palette(cfg.general.hud_appearance, cx);
        crate::ui::set_ui_language(lang);
        let listening = matches!(self.state, HudState::Listening { mode: RecordMode::Toggle });
        let processing = matches!(self.state, HudState::Recognizing | HudState::Polishing);
        // The capsule actually centres on the full window width (the root's horizontal padding
        // does not shift that centring — measured on screen), and the tip's absolute `left` is
        // window-0-based, so anchor centres are expressed in that same full-window frame.
        let capsule_left = (WIN_W + 2. * WIN_PAD - CAPSULE_W) / 2.;
        let mut above = div().w_full().flex_1().relative().flex().flex_col().justify_end().items_center();

        let device_note = self
            .st
            .capture
            .as_ref()
            .and_then(|c| c.device_notice())
            .filter(|(_, at)| at.elapsed() < Duration::from_secs(1))
            .map(|(name, _)| dito_core::i18n::fmt::using_model(lang, &name));
        let error_detail = if let HudState::Error { kind, .. } = &self.state {
            Some(crate::ui::error_summary(lang, *kind))
        } else {
            None
        };

        let button_center = CAPSULE_PAD + BTN / 2.;
        let note_taken = device_note.is_some();
        if let Some(note) = device_note.filter(|_| error_detail.is_none()) {
            above = above.child(Self::bubble(&note, &p));
        }
        // Hover-driven tips pop like motion/tip (design 12.01); the off-flip is recorded so each new hover pops again.
        let tip_on =
            |btn: &'static str| listening && self.hover_btn == Some(btn) && error_detail.is_none() && !note_taken;
        // Both tips stay mounted (their pop lives in the inner child, which tweens out of the
        // button on hover and settles back to 0 unseen while off).
        above = above.child(self.button_tip(
            "hud-tip-finish",
            capsule_left + CAPSULE_W - button_center,
            lang.tr(Tr::HudFinish),
            tip_on("hud-finish"),
            &p,
        ));
        above = above.child(self.button_tip(
            "hud-tip-cancel",
            capsule_left + button_center,
            lang.tr(Tr::HudCancel),
            tip_on("hud-cancel"),
            &p,
        ));
        // The error capsule's recovery buttons carry the same tips: close left, retry or settings right.
        if let HudState::Error { action, .. } = &self.state {
            let err_tip_on = |btn: &'static str| {
                error_detail.is_some()
                    && self.hover_btn == Some(btn)
                    && (self.error_reason.position > 0.18 || self.freeze.is_some())
            };
            let (right_id, right_btn, right_text) = match action {
                ErrorAction::Retry => ("hud-tip-retry", "hud-retry", lang.tr(Tr::HudRetry)),
                ErrorAction::OpenSettings => ("hud-tip-settings", "hud-settings", lang.tr(Tr::HudOpenSettings)),
            };
            above = above.child(self.button_tip(
                "hud-tip-close",
                capsule_left + button_center,
                lang.tr(Tr::HudClose),
                err_tip_on("hud-dismiss"),
                &p,
            ));
            above = above.child(self.button_tip(
                right_id,
                capsule_left + CAPSULE_W - button_center,
                right_text,
                err_tip_on(right_btn),
                &p,
            ));
        }

        // Hover zone: the capsule plus the controls above it and the gap between, so the pointer can travel up.
        let mut zone = div().id("hud-zone").flex().flex_col().items_center().gap(px(6.));
        let tip_showing = self.hover_btn.is_some();
        let chip_on = listening && !tip_showing && self.hover_zone;
        let pill_on = processing && self.hover_zone;
        let chip = self.language_chip(&p, cx);
        if let Some(chip) = crate::ui::hover_pop_scaled(ElementId::from("hud-lang-chip"), chip_on, 0.8, chip) {
            zone = zone.child(chip);
        }
        if let Some(pill) = crate::ui::hover_pop(ElementId::from("hud-stop-pill"), pill_on, || {
            self.pill("hud-stop", &p)
                .on_click(cx.listener(|this, _, _, _| this.send(Command::Stop)))
                .child(hud_icon(HudIc::Stop, tx(11.), hud_color(&p, "#DD202C", "#FF7972")))
                .child(text(
                    lang.tr(Tr::HudStop).to_string(),
                    tx(12.),
                    FontWeight::NORMAL,
                    hud_color(&p, "#DD202C", "#FF7972"),
                ))
        }) {
            zone = zone.child(pill);
        }
        let zone = zone.child(self.pop_in(self.capsule(p, cx), self.width()));

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
            .pb(px(35. + WIN_PAD))
            .pl(px(WIN_PAD))
            .pr(px(WIN_PAD))
            .gap(px(6.))
            .font_family(crate::ui::cjk_family());
        // Paint the travelling reason behind the capsule; its fixed anchor cannot move the HUD.
        if let Some(msg) = error_detail {
            let progress = self.error_reason.position;
            let y = CAPSULE_TOP - 6. - ERROR_BUBBLE_H + (33. + 12. + ERROR_BUBBLE_H) * progress;
            root = root.child(
                div()
                    .absolute()
                    .top(px(y))
                    .left(px(0.5))
                    .right(px(0.5))
                    .flex()
                    .justify_center()
                    .child(Self::bubble(msg, &p)),
            );
        }
        root = root.child(above).child(zone);
        if let Some((seconds, active)) = countdown {
            let reduced = dito_platform::system::reduce_motion();
            let elapsed =
                self.freeze.unwrap_or_else(|| self.recording_started.map_or(0., |t| t.elapsed().as_secs_f32()));
            let entry = ((elapsed - 240.) / 0.18).clamp(0., 1.);
            let opacity = if active { entry } else { (1. - self.elapsed() / LABEL_FADE_S).clamp(0., 1.) };
            let (bg, border, ink) = if seconds <= 10 {
                (
                    hud_color(&p, "#FFF3D9", "#51452D"),
                    hud_color(&p, "#D9BA77", "#9C8047"),
                    hud_color(&p, "#805A17", "#F1CF83"),
                )
            } else {
                (p.hud_bg, p.hud_border, p.text)
            };
            let pill = div()
                .w(px(52.))
                .h(px(22.))
                .rounded_full()
                .border(px(PILL_HAIRLINE))
                .border_color(border)
                .bg(bg)
                .shadow(surface_shadow(&p, Scale::HUD, false))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    text(format!("{:02}:{:02}", seconds / 60, seconds % 60), tx(12.), FontWeight::NORMAL, ink)
                        .font_features(FontFeatures(Arc::new(vec![("tnum".into(), 1)]))),
                );
            root = root.child(
                div()
                    .absolute()
                    .bottom(px(5. + WIN_PAD - if reduced || !active { 0. } else { (1. - entry) * 4. }))
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .opacity(opacity)
                    .child(pill),
            );
        }
        root
    }
}

#[cfg(test)]
mod pointer_tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn the_lower_error_reason_pauses_dismissal_but_does_not_intercept_clicks() {
        let error = HudState::Error {
            kind: dito_core::error::ErrorKind::Mic,
            detail: String::new(),
            action: ErrorAction::OpenSettings,
        };
        let center = (WIN_W + 2. * WIN_PAD) / 2.;
        let (zone, button, interactive) = pointer_hit(&error, CAPSULE_W, true, center, CAPSULE_TOP + 55.);
        assert!(zone);
        assert!(button.is_none());
        assert!(!interactive);
        assert!(!pointer_hit(&error, CAPSULE_W, true, center, CAPSULE_TOP + 80.).0);
    }

    #[::core::prelude::v1::test]
    fn only_controls_intercept_clicks() {
        let toggle = HudState::Listening { mode: RecordMode::Toggle };
        let hold = HudState::Listening { mode: RecordMode::Hold };
        let center = (WIN_W + 2. * WIN_PAD) / 2.;
        let top = super::super::window::WIN_H + 2. * WIN_PAD - (35. + WIN_PAD) - 33.;
        let button_x = center - CAPSULE_W / 2. + CAPSULE_PAD + BTN / 2.;
        assert!(pointer_hit(&toggle, CAPSULE_W, false, button_x, top + 16.5).2);
        assert!(!pointer_hit(&toggle, CAPSULE_W, false, center, top + 16.5).2);
        assert!(!pointer_hit(&hold, 72., false, button_x, top + 16.5).2);
        assert!(!pointer_hit(&toggle, CAPSULE_W, true, center, top + 50.).2);
        assert!(pointer_hit(&toggle, CAPSULE_W, true, center, top - 20.).2);
        assert!(pointer_hit(&HudState::Recognizing, CAPSULE_W, true, center, top - 20.).2);
        assert!(!pointer_hit(&HudState::Recognizing, CAPSULE_W, true, center + 80., top - 20.).2);
    }
}
