use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::callbacks::ToggleCb;
use super::motion::Motion;
use super::primitives::mix;
use super::tween::{count_flips, spring};
use crate::Palette;

const SWITCH_W: f32 = 42.;
const SWITCH_H: f32 = 24.;
const KNOB: f32 = 18.;
const KNOB_INSET: f32 = 3.;
const SWITCH_TRAVEL: f32 = SWITCH_W - KNOB - 2. * KNOB_INSET;
const SWITCH_MS: u64 = 380;

pub fn switch(id: impl Into<ElementId>, p: &Palette, on: bool, on_toggle: ToggleCb) -> Stateful<Div> {
    let id: ElementId = id.into();
    let flips = count_flips(&id, on, Duration::from_millis(SWITCH_MS));
    let (track_off, track_on, knob_off, knob_on) = (p.toggle_off, p.primary, p.knob_off, p.knob_on);
    let knob_shadow = p.switch_knob_shadow;
    let track_at = move |f: f32| {
        div().absolute().top(px(0.)).left(px(0.)).size_full().rounded_full().bg(mix(
            track_off,
            track_on,
            f.clamp(0., 1.),
        ))
    };
    let knob_at = move |f: f32| {
        div()
            .absolute()
            .top(px(KNOB_INSET))
            .left(px(KNOB_INSET + SWITCH_TRAVEL * f))
            .size(px(KNOB))
            .rounded_full()
            .bg(mix(knob_off, knob_on, f.clamp(0., 1.)))
            .shadow(vec![BoxShadow {
                color: knob_shadow,
                offset: point(px(0.), px(1.)),
                blur_radius: px(3.),
                spread_radius: px(0.),
                inset: false,
            }])
    };
    let settled = if on { 1. } else { 0. };
    let root = div()
        .id(id.clone())
        .w(px(SWITCH_W))
        .h(px(SWITCH_H))
        .flex_none()
        .relative()
        .cursor_pointer()
        .on_click(move |_, w, cx| on_toggle(!on, w, cx));
    if flips == 0 {
        return root.child(track_at(settled)).child(knob_at(settled));
    }
    let animation = || Animation::new(Duration::from_millis(SWITCH_MS));
    let motion = Motion::system();
    // The knob rides the spring (a hair of overshoot); the colour just follows the clamped position.
    let value = move |t: f32| {
        let q = spring(t, motion);
        if on { q } else { 1. - q }
    };
    root.child(track_at(settled).with_animation(
        (id.clone(), SharedString::from(format!("track-{flips}"))),
        animation(),
        move |_, t| track_at(value(t)),
    ))
    .child(knob_at(settled).with_animation(
        (id, SharedString::from(format!("knob-{flips}"))),
        animation(),
        move |_, t| knob_at(value(t)),
    ))
}
