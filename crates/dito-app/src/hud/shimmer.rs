//! The label of the processing stages.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{self, Palette};
use crate::ui::{self, Motion, text};

use super::Scale;

const SWEEP_MS: f32 = 2500.;
const SWEEP_TRAVEL: f32 = 1100.;

/// The crest rests outside the label for 1400 ms between sweeps.
fn sweep_at(t: f32, total: f32) -> f32 {
    let half = total * 0.25;
    let travel = (t.max(0.) * 1000. % SWEEP_MS) / SWEEP_TRAVEL;
    if travel < 1. { -half + travel * (total + 2. * half) } else { total + half }
}

/// The label of a capsule that waits for the user: the processing label at rest, with no sweep.
pub(super) fn static_label(s: &str, scale: Scale, p: &Palette) -> Div {
    let size = scale.tx(12.);
    ui::label(s, size, FontWeight::NORMAL, theme::hud_color(p, "#444139", "#F7F5F0"))
}

/// The capsule centers the row; nothing is measured or given a computed width, so every language centers exactly at
/// every display scale. The crest is a moving clip window over stacked copies of the same row; window edges snap to
/// whole points and stay well above a point wide, so GPUI's paint culling cannot drop them.
pub(super) fn shimmer_label(s: &str, t: f32, scale: Scale, p: &Palette, motion: Motion, command: bool) -> Div {
    let px = |v: f32| scale.px(v);
    let size = scale.tx(12.);
    let base = theme::hud_color(p, "#8A867B", "#C2BDB2");
    let hot = theme::hud_color(p, "#575247", "#FFFFFF");
    let shoulder = if command { theme::hud_command_shoulder(p) } else { theme::hud_color(p, "#706C61", "#FFECBD") };
    // Only the crest's travel math needs a width; layout never uses it.
    // text_width already returns scaled pixels: do not scale the measurement a second time.
    let total = ui::text_width(s, size, FontWeight::NORMAL);
    // One plain text run whose width is the run itself, never a measurement.
    let row = |color: Hsla| {
        div()
            .flex()
            .items_center()
            .h(px(19.))
            .child(text(s.to_string(), size, FontWeight::NORMAL, color).line_height(relative(1.6)))
    };
    // Clamped to the label so a partially arrived or leaving band shows only the part that overlaps the glyphs.
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
