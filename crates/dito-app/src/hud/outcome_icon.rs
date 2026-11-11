//! The icon of the finished stage.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::hex;
use crate::ui::Motion;

use super::Scale;
use super::timing::{self, OUTCOME_ICON_S};

const CHECK_ICON: f32 = 18.;
pub(super) const COPY_ICON: f32 = 14.;

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

fn check_outline(progress: f32, origin: Point<Pixels>, scale: Scale) -> Option<Path<Pixels>> {
    if progress <= 0. {
        return None;
    }
    let points = check_trace(progress);
    // These equally wound round-ended contours overlap at the bend. Even-odd filling subtracts that intersection and
    // cuts a hole through the check; nonzero fills the union.
    let mut path =
        PathBuilder::fill().with_style(PathStyle::Fill(FillOptions::default().with_fill_rule(FillRule::NonZero)));
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let angle = (b.1 - a.1).atan2(b.0 - a.0);
        for end in 0..2 {
            let center = if end == 0 { b } else { a };
            for i in 0..=16 {
                let theta = angle - std::f32::consts::FRAC_PI_2 + (end as f32 + i as f32 / 16.) * std::f32::consts::PI;
                let point = point(
                    origin.x + scale.px(center.0 + theta.cos() * CHECK_ICON / 24.),
                    origin.y + scale.px(center.1 + theta.sin() * CHECK_ICON / 24.),
                );
                if end == 0 && i == 0 {
                    path.move_to(point);
                } else {
                    path.line_to(point);
                }
            }
        }
        path.close();
    }
    path.build().ok()
}

fn drawn_check(progress: f32, scale: Scale) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            if let Some(path) = check_outline(progress, bounds.origin, scale) {
                window.paint_path(path, hex("#FFFFFF"));
            }
        },
    )
    .size(scale.px(CHECK_ICON))
    .flex_none()
    .into_any_element()
}

#[derive(Clone, Copy)]
pub(super) enum Outcome {
    Inserted,
    Copied,
}

/// The halo is fixed, so it is cached in the owning window's element state.
fn check_halo(alpha: f32, scale: Scale) -> AnyElement {
    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="42" height="42" viewBox="-12 -12 42 42">
<defs><filter id="soft" x="-100%" y="-100%" width="300%" height="300%"><feGaussianBlur stdDeviation="4"/></filter><filter id="core" x="-100%" y="-100%" width="300%" height="300%"><feGaussianBlur stdDeviation="1.5"/></filter></defs>
<g fill="none" stroke="white" stroke-linecap="round" stroke-linejoin="round"><path d="M3 9.75L6.75 13.5L15 5.25" stroke-width="3.5" opacity=".35" filter="url(#soft)"/><path d="M3 9.75L6.75 13.5L15 5.25" stroke-width="2.5" opacity=".55" filter="url(#core)"/></g></svg>"##;
    div()
        .absolute()
        .inset_0()
        .opacity(alpha)
        .child(
            canvas(
                |_, window, cx| {
                    window.with_global_id("hud-check-halo".into(), |id, window| {
                        window.with_element_state(id, |cached: Option<Option<std::sync::Arc<RenderImage>>>, _| {
                            let image = cached
                                .unwrap_or_else(|| cx.svg_renderer().render_single_frame(SVG.as_bytes(), 2.).ok());
                            (image.clone(), image)
                        })
                    })
                },
                move |bounds, image, window, _| {
                    if let Some(image) = image {
                        let rect = Bounds::new(
                            bounds.origin - point(scale.px(12.), scale.px(12.)),
                            size(scale.px(42.), scale.px(42.)),
                        );
                        window.paint_image(rect, rect, Corners::all(px(0.)), image, 0, false).ok();
                    }
                },
            )
            .size(scale.px(CHECK_ICON)),
        )
        .into_any_element()
}

pub(super) fn draw(outcome: Outcome, time: Option<f32>, motion: Motion, scale: Scale) -> AnyElement {
    let progress = if motion.is_reduced() { 1. } else { time.map_or(1., |t| (t / OUTCOME_ICON_S).clamp(0., 1.)) };
    if matches!(outcome, Outcome::Inserted) {
        let halo = timing::check_halo(time, motion);
        return div()
            .relative()
            .size(scale.px(CHECK_ICON))
            .flex_none()
            .when(halo > 0., |el| el.child(check_halo(halo, scale)))
            .child(drawn_check(progress, scale))
            .into_any_element();
    }
    let ink = hex("#FFF9EB");
    let (front, back, _) = timing::copied_sheets(time, motion);
    div()
        .relative()
        .size(scale.px(COPY_ICON))
        .flex_none()
        .child(
            div()
                .absolute()
                .inset_0()
                .opacity(back)
                .child(svg().path("icons/hud/copy-back.svg").size(scale.px(COPY_ICON)).text_color(ink)),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .opacity(front)
                .child(svg().path("icons/hud/copy-front.svg").size(scale.px(COPY_ICON)).text_color(ink)),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{Scale, check_outline};
    use gpui_kit::{Pixels, Point, point, px};

    #[test]
    fn the_two_strokes_keep_their_overlapping_bend_filled() {
        let cross = |a: Point<Pixels>, b: Point<Pixels>, p: Point<Pixels>| {
            f32::from(b.x - a.x) * f32::from(p.y - a.y) - f32::from(b.y - a.y) * f32::from(p.x - a.x)
        };
        for progress in [0.4, 0.7, 1.] {
            let path = check_outline(progress, point(px(0.), px(0.)), Scale::DESIGN).unwrap();
            let sample = point(px(6.75), px(13.4));
            let filled = path.vertices.as_chunks::<3>().0.iter().any(|triangle| {
                let [a, b, c] = std::array::from_fn(|i| triangle[i].xy_position);
                let sides = [cross(a, b, sample), cross(b, c, sample), cross(c, a, sample)];
                cross(a, b, c).abs() > 0.0001
                    && (sides.iter().all(|s| *s >= -0.0001) || sides.iter().all(|s| *s <= 0.0001))
            });
            assert!(filled, "check lost its bend at progress {progress}");
        }
    }
}
