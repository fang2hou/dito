//! The label of the processing stages: a stable text run with a narrow crest crossing it.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{self, Palette};
use crate::ui::{self, Motion, text};

use super::Scale;

const SWEEP_MS: f32 = 2500.;
const SWEEP_TRAVEL: f32 = 1100.;

/// A narrow crest crosses the label, then rests outside it for 1400 ms.
fn sweep_at(t: f32, total: f32) -> f32 {
    let half = total * 0.25;
    let travel = (t.max(0.) * 1000. % SWEEP_MS) / SWEEP_TRAVEL;
    if travel < 1. { -half + travel * (total + 2. * half) } else { total + half }
}

/// One stable label with a narrow crest; no shadow, font-weight change or baseline movement.
/// The row hugs its content (a single text run when there is no tracking, per-glyph items with a
/// gap otherwise) and the capsule centers it — nothing is measured or given a computed width, so
/// every language centers exactly on every display scale. The crest is a moving clip window over
/// stacked copies of that same row; window edges snap to whole points and stay well above a point
/// wide, so GPUI's paint culling cannot drop them either.
pub(super) fn shimmer_label(s: &str, t: f32, scale: Scale, p: &Palette, motion: Motion) -> Div {
    let px = |v: f32| scale.px(v);
    let size = scale.tx(12.);
    let track = size * ui::tracking_for(s, 0.08);
    let base = theme::hud_color(p, "#6A6455", "#C2BDB2");
    let hot = theme::hud_color(p, "#24221D", "#FFFFFF");
    let shoulder = theme::hud_color(p, "#444139", "#FFECBD");
    let chars: Vec<char> = s.chars().collect();
    // Only the crest's travel math needs a width; layout never uses it.
    // text_width already returns scaled pixels: do not scale the measurements a second time.
    let ws: Vec<f32> = chars.iter().map(|c| ui::text_width(&c.to_string(), size, FontWeight::NORMAL)).collect();
    let total = ws.iter().sum::<f32>() + track * chars.len().saturating_sub(1) as f32;
    let row = |color: Hsla| {
        let mut r = div().flex().items_center().gap(gpui_kit::px(track)).h(px(19.));
        if track == 0. {
            // No tracking: one plain text run — its width is the run itself, never a measurement.
            r = r.child(text(s.to_string(), size, FontWeight::NORMAL, color).line_height(relative(1.6)));
        } else {
            for c in &chars {
                r = r
                    .child(text(c.to_string(), size, FontWeight::NORMAL, color).line_height(relative(1.6)).flex_none());
            }
        }
        r
    };
    // The crest: a soft light band crossing the label. `from`/`to` are clamped to the label so a
    // partially arrived or leaving band shows exactly the part that overlaps the glyphs.
    let band = |from: f32, to: f32, color: Hsla| {
        let from = from.round().clamp(-2., total);
        let to = to.round().clamp(from + 2., total + 2.);
        div()
            .absolute()
            .top_0()
            .left(gpui_kit::px(from))
            .w(gpui_kit::px(to - from))
            .h(px(19.))
            .overflow_hidden()
            .child(div().ml(gpui_kit::px(-from)).child(row(color)))
    };
    let mut label = div().relative().h(px(19.)).flex_none().child(row(base));
    if !motion.is_reduced() && (t.max(0.) * 1000. % SWEEP_MS) < SWEEP_TRAVEL {
        let sweep = sweep_at(t, total);
        let half = total * 0.25;
        label = label
            .child(band(sweep - half, sweep + half, shoulder))
            .child(band(sweep - half * 0.62, sweep + half * 0.62, ui::mix_rgb(shoulder, hot, 0.5)))
            .child(band(sweep - half * 0.3, sweep + half * 0.3, hot));
    }
    label
}
