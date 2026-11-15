//! The HUD's processing label: an adapter over the UI kit's shimmer sweep (`ui::shimmer_text`).

use gpui_kit::*;

use crate::theme::{self, Palette};
use crate::ui::{self, Motion, ShimmerInk};

use super::Scale;

/// The label of a capsule that waits for the user: the processing label at rest, with no sweep.
pub(super) fn static_label(s: &str, scale: Scale, p: &Palette) -> Div {
    let size = scale.tx(12.);
    ui::label(s, size, FontWeight::NORMAL, theme::hud_color(p, "#444139", "#F7F5F0"))
}

/// The processing label; command capsules tint the crest's shoulder.
pub(super) fn shimmer_label(s: &str, t: f32, scale: Scale, p: &Palette, motion: Motion, command: bool) -> Div {
    let mut ink = ShimmerInk::processing(p);
    if command {
        ink.shoulder = theme::hud_command_shoulder(p);
    }
    ui::shimmer_text(s, t, scale.tx(12.), FontWeight::NORMAL, scale.px(19.), ink, motion)
}
