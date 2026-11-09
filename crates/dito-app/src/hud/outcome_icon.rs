//! The icon of the finished stage: a drawn-on check for inserted text, a stacked pair of sheets for copied.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::Ic;
use crate::theme::hex;
use crate::ui::{Motion, icon};

use super::Scale;
use super::timing::OUTCOME_ICON_S;

/// Centered state glyphs in design points.
const CHECK_ICON: f32 = 18.;
pub(super) const COPY_ICON: f32 = 14.;

/// A constant-speed tip travels along the two segments, following the check's actual centerline.
fn check_trace(progress: f32) -> Vec<(f32, f32)> {
    let [a, b, c] = [(4_f32, 13_f32), (9., 18.), (20., 7.)].map(|(x, y)| (x * CHECK_ICON / 24., y * CHECK_ICON / 24.));
    let ab = (b.0 - a.0).hypot(b.1 - a.1);
    let bc = (c.0 - b.0).hypot(c.1 - b.1);
    let length = progress.clamp(0., 1.) * (ab + bc);
    if length <= ab {
        let t = length / ab;
        vec![a, (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)]
    } else {
        let t = (length - ab) / bc;
        vec![a, b, (b.0 + (c.0 - b.0) * t, b.1 + (c.1 - b.1) * t)]
    }
}

fn drawn_check(progress: f32, scale: Scale) -> AnyElement {
    if progress >= 1. {
        return icon(Ic::Check, scale.tx(CHECK_ICON), hex("#FFFFFF")).into_any_element();
    }
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            if progress <= 0. {
                return;
            }
            let points = check_trace(progress);
            for pair in points.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let angle = (b.1 - a.1).atan2(b.0 - a.0);
                let mut path = PathBuilder::fill();
                // A round-ended stroke for each segment, including the bend. Its thickness
                // matches the canonical SVG, so the settled icon never changes visual weight.
                for end in 0..2 {
                    let center = if end == 0 { b } else { a };
                    for i in 0..=16 {
                        let theta =
                            angle - std::f32::consts::FRAC_PI_2 + (end as f32 + i as f32 / 16.) * std::f32::consts::PI;
                        let point = point(
                            bounds.origin.x + scale.px(center.0 + theta.cos() * CHECK_ICON / 24.),
                            bounds.origin.y + scale.px(center.1 + theta.sin() * CHECK_ICON / 24.),
                        );
                        if end == 0 && i == 0 {
                            path.move_to(point);
                        } else {
                            path.line_to(point);
                        }
                    }
                }
                path.close();
                if let Ok(path) = path.build() {
                    window.paint_path(path, hex("#FFFFFF"));
                }
            }
        },
    )
    .size(scale.px(CHECK_ICON))
    .flex_none()
    .into_any_element()
}

/// Which finished stage the icon belongs to.
#[derive(Clone, Copy)]
pub(super) enum Outcome {
    Inserted,
    Copied,
}

pub(super) fn draw(outcome: Outcome, time: Option<f32>, motion: Motion, scale: Scale) -> AnyElement {
    let progress = if motion.is_reduced() { 1. } else { time.map_or(1., |t| (t / OUTCOME_ICON_S).clamp(0., 1.)) };
    if matches!(outcome, Outcome::Inserted) {
        return drawn_check(progress, scale);
    }
    let ink = hex("#FFF9EB");
    if progress >= 1. {
        return icon(Ic::Copy, scale.tx(COPY_ICON), ink).into_any_element();
    }
    let front = (progress / 0.375).clamp(0., 1.);
    let back = ((progress - 0.3125) / 0.6875).clamp(0., 1.);
    let ease = |u: f32| 1. - (1. - u).powi(3);
    div()
        .relative()
        .size(scale.px(COPY_ICON))
        .flex_none()
        .child(
            div()
                .absolute()
                .left(scale.px(-0.9 * (1. - ease(back))))
                .top(scale.px(0.9 * (1. - ease(back))))
                .size(scale.px(COPY_ICON))
                .opacity(ease(back))
                .child(svg().path("icons/hud/copy-back.svg").size(scale.px(COPY_ICON)).text_color(ink)),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .top(scale.px(0.65 * (1. - ease(front))))
                .size(scale.px(COPY_ICON))
                .opacity(ease(front))
                .child(svg().path("icons/hud/copy-front.svg").size(scale.px(COPY_ICON)).text_color(ink)),
        )
        .into_any_element()
}
