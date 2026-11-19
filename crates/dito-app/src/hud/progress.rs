//! The progress fill inside the capsule.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{self, Palette};
use crate::ui::hex_alpha;

use super::Scale;
use super::capsule::{CAPSULE_H, HAIRLINE};

pub(super) const INNER_H: f32 = CAPSULE_H - 2. * HAIRLINE;

fn inner_w(w: f32) -> f32 {
    w - 2. * HAIRLINE
}

/// GPUI clips to rectangles only, so the progress fill is drawn as a polygon that follows the capsule's rounded ends
/// rather than as a clipped box. The outline is the top edge from left to right, then the bottom edge back over the
/// same columns.
fn capsule_slice(w: f32, h: f32, x0: f32, x1: f32) -> Vec<(f32, f32)> {
    let r = h / 2.;
    let half = |x| capsule_half_height(w, h, x);
    let on_straight_middle = |x: f32| x >= r && x <= w - r;
    let (x0, x1) = (x0.clamp(0., w), x1.clamp(0., w));
    let mut xs = vec![x0];
    let mut x = (x0 / 0.5).floor() * 0.5 + 0.5;
    while x < x1 {
        // A column whose two neighbors are on the straight middle lies on the line between them.
        if !(on_straight_middle(x - 0.5) && on_straight_middle(x + 0.5)) {
            xs.push(x);
        }
        x += 0.5;
    }
    xs.push(x1);
    let mut pts: Vec<(f32, f32)> = xs.iter().map(|&x| (x, r - half(x))).collect();
    pts.extend(xs.iter().rev().map(|&x| (x, r + half(x))));
    pts
}

fn capsule_half_height(w: f32, h: f32, x: f32) -> f32 {
    let r = h / 2.;
    let x = x.clamp(0., w);
    let d = if x < r {
        r - x
    } else if x > w - r {
        x - (w - r)
    } else {
        0.
    };
    (r * r - d * d).max(0.).sqrt()
}

fn slice_canvas(w: f32, x0: f32, x1: f32, fill: impl Into<Background> + 'static, scale: Scale) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let outline = capsule_slice(inner_w(w), INNER_H, x0, x1);
            let at = |i: usize| {
                let (x, y) = outline[i];
                point(bounds.origin.x + gpui_kit::px(x * scale.0), bounds.origin.y + gpui_kit::px(y * scale.0))
            };
            // The outline is already cut into columns, so each one is pushed as two triangles instead of running the
            // fill tessellator on every frame.
            let solid = point(0., 1.);
            let last = outline.len() - 1;
            let mut path = Path::new(at(0));
            for column in 0..last / 2 {
                let (top, next_top) = (at(column), at(column + 1));
                let (bottom, next_bottom) = (at(last - column), at(last - column - 1));
                path.push_triangle((top, next_top, next_bottom), (solid, solid, solid));
                path.push_triangle((top, next_bottom, bottom), (solid, solid, solid));
            }
            window.paint_path(path, fill);
        },
    )
    .absolute()
    .inset_0()
}

pub(super) fn progress_layer(
    frac: f32,
    w: f32,
    scale: Scale,
    p: &Palette,
    halo_strength: f32,
    command: bool,
) -> impl IntoElement {
    let x = inner_w(w) * frac.clamp(0., 1.);
    let (fill, edge) = if command {
        (theme::hud_command_progress_fill(p), theme::hud_command_progress_edge(p))
    } else {
        (theme::hud_color(p, "#F6DA97", "#827554"), theme::hud_color(p, "#AA8C40", "#E0C47D"))
    };
    let halo = hex_alpha(edge, halo_strength * if p.dark { 0.20 } else { 0.14 });
    let half_height =
        capsule_half_height(inner_w(w), INNER_H, x - 0.5).min(capsule_half_height(inner_w(w), INNER_H, x + 0.5));
    div()
        .absolute()
        .inset_0()
        .child(slice_canvas(w, 0., x, fill, scale))
        .child(slice_canvas(
            w,
            x - 4.,
            x,
            linear_gradient(90., linear_color_stop(hex_alpha(halo, 0.), 0.), linear_color_stop(halo, 1.)),
            scale,
        ))
        .child(slice_canvas(
            w,
            x,
            x + 4.,
            linear_gradient(90., linear_color_stop(halo, 0.), linear_color_stop(hex_alpha(halo, 0.), 1.)),
            scale,
        ))
        // Paint the crisp front as a quad: path antialiasing spreads subpixel-width paths
        // into a faint blur, which swallowed the front inside its own halo.
        .child(
            div()
                .absolute()
                .left(gpui_kit::px((x - 0.5) * scale.0))
                .top(gpui_kit::px((INNER_H / 2. - half_height) * scale.0))
                .w(gpui_kit::px(1. * scale.0))
                .h(gpui_kit::px(2. * half_height * scale.0))
                .bg(edge),
        )
}

#[cfg(test)]
mod tests {
    use super::{INNER_H, capsule_half_height, capsule_slice, inner_w};
    use crate::hud::capsule::{CAPSULE_W, NARROW_W};

    #[test]
    fn the_progress_edge_is_a_vertical_line_even_inside_the_rounded_ends() {
        let inner = inner_w(CAPSULE_W);
        for x in [2., 6., 13., 41., 70., 78., inner - 0.5] {
            let pts = capsule_slice(inner, INNER_H, 0., x);
            let right: Vec<_> = pts.iter().filter(|p| (p.0 - x).abs() < 1e-4).collect();
            assert_eq!(right.len(), 2, "exactly the top and bottom point on the edge at x={x}");
            let (top, bottom) = (right[0].1.min(right[1].1), right[0].1.max(right[1].1));
            assert!(top >= 0. && bottom <= INNER_H, "inside the capsule at x={x}");
        }
        // Past the end cap the cut is shorter than the full height: the capsule's curve, not a box.
        let pts = capsule_slice(inner, INNER_H, 0., inner - 1.);
        let h = pts
            .iter()
            .filter(|p| (p.0 - (inner - 1.)).abs() < 1e-4)
            .map(|p| p.1)
            .fold((f32::MAX, 0f32), |a, y| (a.0.min(y), a.1.max(y)));
        assert!(h.1 - h.0 < INNER_H * 0.8);
    }

    #[test]
    fn the_narrow_capsule_cuts_its_rounded_ends_at_its_own_width() {
        // Hold mode's 72 pt capsule: the cut near its right end follows the curve of that capsule, not of a 105 pt
        // one.
        let narrow = inner_w(NARROW_W);
        let near_end = narrow - 1.;
        let height_at = |w: f32, x: f32| {
            let pts = capsule_slice(w, INNER_H, 0., x);
            let ys: Vec<f32> = pts.iter().filter(|p| (p.0 - x).abs() < 1e-4).map(|p| p.1).collect();
            ys.iter().cloned().fold(f32::MIN, f32::max) - ys.iter().cloned().fold(f32::MAX, f32::min)
        };
        assert!(height_at(narrow, near_end) < INNER_H * 0.8, "short near the narrow capsule's end");
        assert!(
            (height_at(inner_w(CAPSULE_W), near_end) - INNER_H).abs() < 1e-3,
            "full height mid-way along the wide one"
        );
    }

    #[test]
    fn skipping_columns_on_the_straight_middle_leaves_the_outline_unchanged() {
        let inner = inner_w(CAPSULE_W);
        for (x0, x1) in [(0., inner), (3.2, 61.7), (40., 44.)] {
            let outline = capsule_slice(inner, INNER_H, x0, x1);
            let top = &outline[..outline.len() / 2];
            assert!(top.len() < 70, "the straight middle keeps only its ends from {x0} to {x1}");
            let mut x = (x0 / 0.5).floor() * 0.5 + 0.5;
            while x < x1 {
                let after = top.partition_point(|p| p.0 < x);
                let (a, b) = (top[after - 1], top[after]);
                let y = a.1 + (b.1 - a.1) * (x - a.0) / (b.0 - a.0);
                let expected = INNER_H / 2. - capsule_half_height(inner, INNER_H, x);
                assert!((y - expected).abs() < 1e-4, "the top edge at x={x} from {x0} to {x1}");
                x += 0.5;
            }
        }
    }
}
