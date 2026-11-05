//! The discrete HUD-height control. The labels stay qualitative; the state is an accessible slider.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::i18n::Tr;
use gpui_kit::base::slider::{Slider, SliderIndicator, SliderState, SliderThumb, SliderTrack};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{Ctx, watch_config};
use crate::state::AppState;
use crate::ui::text;

const MIN: i8 = -2;
const MAX: i8 = 12;
const TRACK: f32 = 220.;
const KNOB: f32 = 24.;
/// Thumb sizes from design 12.03: it grows a little under the pointer and more while pressed.
const KNOB_HOVER: f32 = 26.;
const KNOB_PRESSED: f32 = 32.;
const TRAVEL: f32 = TRACK - KNOB;

/// A damped, time-based transition; the target may change without restarting the motion.
fn spring_step(x: f32, v: f32, target: f32, dt: f32) -> (f32, f32) {
    if dt <= 0. {
        return (x, v);
    }
    if dt > 0.25 {
        return (target, 0.);
    }
    let omega = 23_f32;
    let zeta = 0.82_f32;
    let decay = omega * zeta;
    let wd = omega * (1. - zeta * zeta).sqrt();
    let d = x - target;
    let b = (v + decay * d) / wd;
    let (sin, cos) = (wd * dt).sin_cos();
    let e = (-decay * dt).exp();
    let wave = d * cos + b * sin;
    (target + e * wave, e * (wd * (b * cos - d * sin) - decay * wave))
}

pub(super) struct HudHeightSlider {
    st: Arc<AppState>,
    slider: Entity<SliderState>,
    focus: FocusHandle,
    hovered: bool,
    pressed: bool,
    dragging: bool,
    changes_in_press: u32,
    position: f32,
    velocity: f32,
    size: f32,
    size_velocity: f32,
    tick: Instant,
    config_level: i8,
}

impl HudHeightSlider {
    #[cfg(feature = "screenshots")]
    pub(super) fn bounds(&self, cx: &App) -> Bounds<Pixels> {
        self.slider.read(cx).bounds()
    }

    pub fn new(st: Arc<AppState>, cx: &mut Context<Self>) -> Self {
        watch_config(&st, cx);
        let level = st.cfg.get().general.hud_height_level();
        let slider =
            cx.new(|_| SliderState::new().min(MIN as f32).max(MAX as f32).step(1.).default_value(level as f32));
        cx.observe(&slider, |this, slider, cx| {
            let level = (slider.read(cx).value().end().round() as i8).clamp(MIN, MAX);
            if this.pressed {
                this.changes_in_press += 1;
                this.dragging = this.changes_in_press > 1;
            }
            if this.st.cfg.get().general.hud_height_level() != level
                && let Err(e) = this.st.cfg.update(|cfg| cfg.general.hud_height_level = Some(level))
            {
                tracing::warn!("could not save HUD height: {e}");
            }
            cx.notify();
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(16)).await;
                if this
                    .update(cx, |this, cx| {
                        let now = Instant::now();
                        let dt = now.duration_since(this.tick).as_secs_f32();
                        this.tick = now;
                        let target = (this.slider.read(cx).value().end() - MIN as f32) / (MAX - MIN) as f32;
                        let size_target = if this.pressed {
                            KNOB_PRESSED
                        } else if this.hovered {
                            KNOB_HOVER
                        } else {
                            KNOB
                        };
                        let old = (this.position, this.size);
                        if this.dragging || dito_platform::system::reduce_motion() {
                            this.position = target;
                            this.velocity = 0.;
                        } else {
                            (this.position, this.velocity) = spring_step(this.position, this.velocity, target, dt);
                        }
                        if dito_platform::system::reduce_motion() {
                            // Reduce Motion keeps the thumb's size; only color and shadow respond.
                            this.size = KNOB;
                            this.size_velocity = 0.;
                        } else {
                            (this.size, this.size_velocity) =
                                spring_step(this.size, this.size_velocity, size_target, dt);
                        }
                        if (this.position - target).abs() < 0.0001 && this.velocity.abs() < 0.001 {
                            this.position = target;
                            this.velocity = 0.;
                        }
                        let size_target = if dito_platform::system::reduce_motion() { KNOB } else { size_target };
                        if (this.size - size_target).abs() < 0.01 && this.size_velocity.abs() < 0.1 {
                            this.size = size_target;
                            this.size_velocity = 0.;
                        }
                        if old != (this.position, this.size) {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self {
            st,
            slider,
            focus: cx.focus_handle(),
            hovered: false,
            pressed: false,
            dragging: false,
            changes_in_press: 0,
            position: (level - MIN) as f32 / (MAX - MIN) as f32,
            velocity: 0.,
            size: KNOB,
            size_velocity: 0.,
            tick: Instant::now(),
            config_level: level,
        }
    }
}

impl Render for HudHeightSlider {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cfg = self.st.cfg.get();
        let level = cfg.general.hud_height_level() as f32;
        if self.config_level != level as i8 {
            if self.slider.read(cx).value().end() == level {
                self.config_level = level as i8;
            } else if !self.pressed {
                self.slider.update(cx, |slider, cx| slider.set_value(level, window, cx));
                self.config_level = level as i8;
            }
        }
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        let center = KNOB / 2. + TRAVEL * self.position.clamp(0., 1.);
        let knob_color = if self.hovered { p.slider_thumb_hover } else { p.knob_on };
        // The pressed knob lifts off the track: a deeper, wider shadow that scales with the
        // press expansion, so the enlarged edge stays clearly readable against the fill.
        let press = if dito_platform::system::reduce_motion() {
            if self.pressed { 1. } else { 0. }
        } else {
            ((self.size - KNOB_HOVER) / (KNOB_PRESSED - KNOB_HOVER)).clamp(0., 1.)
        };
        let shadow = vec![BoxShadow {
            color: crate::ui::mix(p.slider_thumb_shadow, p.slider_thumb_shadow_pressed, press),
            offset: point(px(0.), px(1. + 2. * press)),
            blur_radius: px(2. + 6. * press),
            spread_radius: px(0.),
            inset: false,
        }];
        let size = self.size.clamp(KNOB, KNOB_PRESSED + 2.);
        let stretch = if dito_platform::system::reduce_motion() {
            0.
        } else {
            (self.velocity.abs() * TRAVEL * 0.003 + self.size_velocity.abs() * 0.004).min(2.)
        };
        let knob_w = size + stretch;
        let knob_h = size - stretch * 0.4;
        let focus = self.focus.clone();
        let slider_for_keys = self.slider.clone();
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(text(c.t(Tr::GeneralHudLower), 12., FontWeight::NORMAL, p.text))
            .child(
                div()
                    .id("hud-height-input")
                    .track_focus(&self.focus)
                    .w(px(TRACK))
                    .h(px(36.))
                    .capture_any_mouse_down(cx.listener(move |this, _, window, cx| {
                        this.pressed = true;
                        this.dragging = false;
                        this.changes_in_press = 0;
                        focus.focus(window, cx);
                        cx.notify();
                    }))
                    .capture_any_mouse_up(cx.listener(|this, _, _, cx| {
                        this.pressed = false;
                        this.dragging = false;
                        cx.notify();
                    }))
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.pressed = false;
                            this.dragging = false;
                            cx.notify();
                        }),
                    )
                    .on_key_down(cx.listener(move |_, event: &KeyDownEvent, window, cx| {
                        let delta = match event.keystroke.key.as_str() {
                            "left" | "down" => -1.,
                            "right" | "up" => 1.,
                            _ => return,
                        };
                        slider_for_keys.update(cx, |s, cx| {
                            let v = (s.value().end() + delta).clamp(MIN as f32, MAX as f32);
                            s.set_value(v, window, cx);
                        });
                        cx.stop_propagation();
                    }))
                    .child(
                        Slider::new(&self.slider)
                            .accessibility_label(c.t(Tr::GeneralHudHeight))
                            .w(px(TRACK))
                            .h(px(36.))
                            .flex()
                            .items_center()
                            .child(
                                SliderTrack::new(&self.slider)
                                    .relative()
                                    .w(px(TRACK))
                                    .h(px(KNOB))
                                    .rounded_full()
                                    .bg(p.toggle_off)
                                    .cursor_pointer()
                                    .child(
                                        SliderIndicator::new(&self.slider)
                                            .absolute()
                                            .left(px(KNOB / 2.))
                                            .top_0()
                                            .w(px(TRAVEL))
                                            .h(px(KNOB)),
                                    )
                                    // Fill = a rounded left cap plus a square-edged body ending on the thumb's
                                    // centre line; empty at the lowest level. gpui's overflow clip is
                                    // rectangular, so like the capsule's progress layer the rounding is drawn,
                                    // not clipped.
                                    .when(self.position > 0.001, |track| {
                                        track
                                            .child(
                                                div()
                                                    .absolute()
                                                    .left_0()
                                                    .top_0()
                                                    .w(px(center.min(KNOB)))
                                                    .h(px(KNOB))
                                                    .rounded_full()
                                                    .bg(p.primary),
                                            )
                                            .child(
                                                div()
                                                    .absolute()
                                                    .left(px(KNOB / 2.))
                                                    .top_0()
                                                    .w(px((center - KNOB / 2.).max(0.)))
                                                    .h(px(KNOB))
                                                    .bg(p.primary),
                                            )
                                    })
                                    .child(
                                        SliderThumb::new(&self.slider)
                                            .absolute()
                                            .left(px(center - KNOB / 2.))
                                            .top_0()
                                            .size(px(KNOB))
                                            .on_hover(cx.listener(|this, over: &bool, _, cx| {
                                                if this.hovered != *over {
                                                    this.hovered = *over;
                                                    cx.notify();
                                                }
                                            }))
                                            .child(
                                                div()
                                                    .absolute()
                                                    .left(px((KNOB - knob_w) / 2.))
                                                    .top(px((KNOB - knob_h) / 2.))
                                                    .w(px(knob_w))
                                                    .h(px(knob_h))
                                                    .rounded_full()
                                                    .bg(knob_color)
                                                    .shadow(shadow),
                                            ),
                                    ),
                            ),
                    ),
            )
            .child(text(c.t(Tr::GeneralHudHigher), 12., FontWeight::NORMAL, p.text))
    }
}
