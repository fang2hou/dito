//! One-line labels with a traveling light crest, shared by the HUD and the dock.

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::motion::Motion;
use super::primitives::mix_rgb;
use super::typography::{text, text_width};
use crate::{Palette, hud_color};

const SWEEP_MS: f32 = 2500.;
const SWEEP_TRAVEL: f32 = 1100.;

/// The inks of a sweeping label: the resting text, the crest's soft edges and its bright center.
pub struct ShimmerInk {
    pub base: Hsla,
    pub shoulder: Hsla,
    pub hot: Hsla,
}

impl ShimmerInk {
    /// The HUD processing label's colors.
    pub fn processing(p: &Palette) -> Self {
        ShimmerInk {
            base: hud_color(p, "#8A867B", "#C2BDB2"),
            shoulder: hud_color(p, "#706C61", "#FFECBD"),
            hot: hud_color(p, "#575247", "#FFFFFF"),
        }
    }
}

/// The crest rests outside the label for 1400 ms between sweeps.
fn sweep_at(t: f32, total: f32) -> f32 {
    let half = total * 0.25;
    let travel = (t.max(0.) * 1000. % SWEEP_MS) / SWEEP_TRAVEL;
    if travel < 1. { -half + travel * (total + 2. * half) } else { total + half }
}

/// The label centers the row; nothing is measured or given a computed width, so every language centers exactly at
/// every display scale. The crest is a moving clip window over stacked copies of the same row; window edges snap to
/// whole points and stay well above a point wide, so GPUI's paint culling cannot drop them.
///
/// `size` is already scaled and `line_box` is the label's box height; `t` is seconds on the caller's clock.
pub fn shimmer_text(
    label: &str,
    t: f32,
    size: f32,
    weight: FontWeight,
    line_box: Pixels,
    ink: ShimmerInk,
    motion: Motion,
) -> Div {
    let ShimmerInk { base, shoulder, hot } = ink;
    // Only the crest's travel math needs a width; layout never uses it.
    // text_width already returns scaled pixels: do not scale the measurement a second time.
    let total = text_width(label, size, weight);
    // One plain text run whose width is the run itself, never a measurement.
    let row = |color: Hsla| {
        div()
            .flex()
            .items_center()
            .h(line_box)
            .child(text(label.to_string(), size, weight, color).line_height(relative(1.6)))
    };
    // Clamped to the label so a partially arrived or leaving band shows only the part that overlaps the glyphs.
    let band = |from: f32, to: f32, color: Hsla| {
        let from = from.round().clamp(-2., total);
        let to = to.round().clamp(from + 2., total + 2.);
        div()
            .absolute()
            .top_0()
            .left(px(from))
            .w(px(to - from))
            .h(line_box)
            .overflow_hidden()
            .child(div().ml(px(-from)).child(row(color)))
    };
    let mut el = div().relative().h(line_box).flex_none().child(row(base));
    if !motion.is_reduced() && (t.max(0.) * 1000. % SWEEP_MS) < SWEEP_TRAVEL {
        let sweep = sweep_at(t, total);
        let half = total * 0.25;
        el = el
            .child(band(sweep - half, sweep + half, shoulder))
            .child(band(sweep - half * 0.62, sweep + half * 0.62, mix_rgb(shoulder, hot, 0.5)))
            .child(band(sweep - half * 0.3, sweep + half * 0.3, hot));
    }
    el
}
