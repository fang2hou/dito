//! The discrete HUD-height control. The labels stay qualitative; the state is an accessible slider.

use std::sync::Arc;
use std::time::Instant;

use dito_core::i18n::Tr;
use gpui_kit::base::slider::{Slider, SliderIndicator, SliderState, SliderThumb, SliderTrack};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::Ctx;
use crate::services;
use crate::state::AppState;
use crate::theme::Palette;
use crate::ui::{Motion, SpringCurve, text};

const MIN: i8 = -2;
const MAX: i8 = 12;
const TRACK: f32 = 220.;
const KNOB: f32 = 24.;
const KNOB_PRESSED: f32 = 32.;
const TRAVEL: f32 = TRACK - KNOB;

/// The target may change without restarting the motion.
fn spring_step(x: f32, v: f32, target: f32, dt: f32) -> (f32, f32) {
    if dt <= 0. {
        return (x, v);
    }
    if dt > 0.25 {
        return (target, 0.);
    }
    let (offset, velocity) = SpringCurve::damped(0.82, 23.).step(x - target, v, dt);
    (target + offset, velocity)
}

pub(super) struct HudHeightSlider {
    st: Arc<AppState>,
    slider: Entity<SliderState>,
    focus: FocusHandle,
    hovered: bool,
    hover_progress: f32,
    hover_from: f32,
    hover_at: Option<Instant>,
    pressed: bool,
    dragging: bool,
    changes_in_press: u32,
    position: f32,
    velocity: f32,
    size: f32,
    size_velocity: f32,
    tick: Option<Instant>,
    config_level: i8,
}

impl HudHeightSlider {
    #[cfg(feature = "screenshots")]
    pub(super) fn bounds(&self, cx: &App) -> Bounds<Pixels> {
        self.slider.read(cx).bounds()
    }

    pub fn new(st: Arc<AppState>, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
        let level = st.cfg.get().general.hud_height_level();
        let slider =
            cx.new(|_| SliderState::new().min(MIN as f32).max(MAX as f32).step(1.).default_value(level as f32));
        cx.observe(&slider, Self::slider_changed).detach();
        Self {
            st,
            slider,
            focus: cx.focus_handle(),
            hovered: false,
            hover_progress: 0.,
            hover_from: 0.,
            hover_at: None,
            pressed: false,
            dragging: false,
            changes_in_press: 0,
            position: (level - MIN) as f32 / (MAX - MIN) as f32,
            velocity: 0.,
            size: KNOB,
            size_velocity: 0.,
            tick: None,
            config_level: level,
        }
    }

    fn slider_changed(&mut self, slider: Entity<SliderState>, cx: &mut Context<Self>) {
        let level = (slider.read(cx).value().end().round() as i8).clamp(MIN, MAX);
        if self.pressed {
            self.changes_in_press += 1;
            self.dragging = self.changes_in_press > 1;
        }
        if self.st.cfg.get().general.hud_height_level() != level {
            services::save_config(&self.st, |cfg| cfg.general.hud_height_level = Some(level));
        }
        cx.notify();
    }

    /// Advance only when this view draws, paced by the display rather than a background timer.
    fn step(&mut self, motion: Motion, cx: &App) -> bool {
        let now = Instant::now();
        let dt = self.tick.map_or(0., |tick| now.duration_since(tick).as_secs_f32());
        let target = (self.slider.read(cx).value().end() - MIN as f32) / (MAX - MIN) as f32;
        if let Some(at) = self.hover_at {
            let t = (now.duration_since(at).as_secs_f32() / 0.16).clamp(0., 1.);
            let target = f32::from(self.hovered);
            self.hover_progress = self.hover_from + (target - self.hover_from) * t * t * (3. - 2. * t);
            if t >= 1. {
                self.hover_at = None;
            }
        }
        // Reduce Motion keeps the resting size even during a press.
        let size_target = if self.pressed && !motion.is_reduced() { KNOB_PRESSED } else { KNOB };
        if self.dragging || motion.is_reduced() {
            self.position = target;
            self.velocity = 0.;
        } else {
            (self.position, self.velocity) = spring_step(self.position, self.velocity, target, dt);
        }
        if motion.is_reduced() {
            self.size = KNOB;
            self.size_velocity = 0.;
        } else {
            (self.size, self.size_velocity) = spring_step(self.size, self.size_velocity, size_target, dt);
        }
        if (self.position - target).abs() < 0.0001 && self.velocity.abs() < 0.001 {
            self.position = target;
            self.velocity = 0.;
        }
        if (self.size - size_target).abs() < 0.01 && self.size_velocity.abs() < 0.1 {
            self.size = size_target;
            self.size_velocity = 0.;
        }
        let active = self.position != target
            || self.velocity != 0.
            || self.size != size_target
            || self.size_velocity != 0.
            || self.hover_at.is_some();
        self.tick = active.then_some(now);
        active
    }

    fn follow_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let level = self.st.cfg.get().general.hud_height_level() as f32;
        if self.config_level == level as i8 {
            return;
        }
        if self.slider.read(cx).value().end() == level {
            self.config_level = level as i8;
        } else if !self.pressed {
            self.slider.update(cx, |slider, cx| slider.set_value(level, window, cx));
            self.config_level = level as i8;
        }
    }

    /// The shadow scales with the press expansion so the enlarged edge stays readable against the fill.
    fn knob(&self, p: &Palette, motion: Motion) -> Div {
        let press = match motion {
            Motion::Reduced => f32::from(self.pressed),
            Motion::Full => ((self.size - KNOB) / (KNOB_PRESSED - KNOB)).clamp(0., 1.),
        };
        let shadow = vec![BoxShadow {
            color: crate::ui::mix(
                p.slider_thumb_shadow.opacity(self.hover_progress),
                p.slider_thumb_shadow_pressed,
                press,
            ),
            offset: point(px(0.), px(1. + 2. * press)),
            blur_radius: px(2. + 6. * press),
            spread_radius: px(0.),
            inset: false,
        }];
        let diameter = self.size.clamp(KNOB, KNOB_PRESSED + 2.);
        let stretch = match motion {
            Motion::Reduced => 0.,
            Motion::Full => (self.velocity.abs() * TRAVEL * 0.003 + self.size_velocity.abs() * 0.004).min(2.),
        };
        let (knob_w, knob_h) = (diameter + stretch, diameter - stretch * 0.4);
        let knob_color =
            crate::ui::mix(p.knob_on, p.slider_thumb_hover, if self.pressed { 0. } else { self.hover_progress });
        // The hit box and layout stay fixed. Only paint geometry changes during the spring.
        div().relative().size(px(KNOB)).child(
            canvas(
                |bounds, _, _| bounds,
                move |region: Bounds<Pixels>, _, window, _| {
                    let bounds = Bounds::new(
                        point(region.center().x - px(knob_w / 2.), region.center().y - px(knob_h / 2.)),
                        size(px(knob_w), px(knob_h)),
                    );
                    let radii = Corners::all(px(knob_w.min(knob_h) / 2.));
                    window.paint_drop_shadows(bounds, radii, &shadow);
                    window.paint_quad(quad(
                        bounds,
                        radii,
                        knob_color,
                        Edges::all(px(0.)),
                        knob_color,
                        BorderStyle::Solid,
                    ));
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size(px(KNOB)),
        )
    }

    /// Separate overlapping cap and body layers would reveal their join when the page fades out.
    fn fill(&self, p: &Palette, track: SliderTrack) -> SliderTrack {
        let center = self.center();
        track.when(self.position > 0.001, |track| {
            track.child(
                div().absolute().left_0().top_0().w(px(center)).h(px(KNOB)).rounded_l(px(KNOB / 2.)).bg(p.primary),
            )
        })
    }

    fn center(&self) -> f32 {
        KNOB / 2. + TRAVEL * self.position.clamp(0., 1.)
    }

    fn thumb(&self, p: &Palette, motion: Motion, cx: &mut Context<Self>) -> SliderThumb {
        SliderThumb::new(&self.slider)
            .absolute()
            .left(px(self.center() - KNOB / 2.))
            .top_0()
            .size(px(KNOB))
            .on_hover(cx.listener(|this, over: &bool, _, cx| {
                if this.hovered != *over {
                    this.hover_from = this.hover_progress;
                    this.hover_at = Some(Instant::now());
                    this.hovered = *over;
                    cx.notify();
                }
            }))
            .child(self.knob(p, motion))
    }

    fn track(&self, p: &Palette, motion: Motion, cx: &mut Context<Self>) -> SliderTrack {
        let track = SliderTrack::new(&self.slider)
            .relative()
            .w(px(TRACK))
            .h(px(KNOB))
            .rounded_full()
            .bg(p.toggle_off)
            .cursor_pointer()
            .child(SliderIndicator::new(&self.slider).absolute().left(px(KNOB / 2.)).top_0().w(px(TRAVEL)).h(px(KNOB)));
        self.fill(p, track).child(self.thumb(p, motion, cx))
    }

    fn input(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let focus = self.focus.clone();
        let slider_for_keys = self.slider.clone();
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
    }
}

impl Render for HudHeightSlider {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.follow_config(window, cx);
        let motion = Motion::system();
        if self.step(motion, cx) {
            window.request_animation_frame();
        }
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(text(c.t(Tr::GeneralHudLower), 12., FontWeight::NORMAL, p.text))
            .child(
                self.input(cx).child(
                    Slider::new(&self.slider)
                        .accessibility_label(c.t(Tr::GeneralHudHeight))
                        .w(px(TRACK))
                        .h(px(36.))
                        .flex()
                        .items_center()
                        .child(self.track(&p, motion, cx)),
                ),
            )
            .child(text(c.t(Tr::GeneralHudHigher), 12., FontWeight::NORMAL, p.text))
    }
}
