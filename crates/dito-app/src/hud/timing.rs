//! The HUD's clocks: progress fill, outcome hold and fade, and the copied label's growth.

use dito_core::controller::HudState;
use dito_core::i18n::{Lang, Tr};
use gpui_kit::*;

use crate::ui::{self, Motion};

use super::capsule::CAPSULE_W;
use super::motion;
use super::outcome_icon::COPY_ICON;
use super::processing_motion::{Fill, ProcessingMotion};

pub(super) const SETTLE_S: f32 = 0.18;

/// Use the shared fill curve with a fixed forecast for synthetic HUD previews.
pub(crate) fn fill_at(state: &HudState, t: f32, carry: Option<f32>) -> Option<f32> {
    let motion = match state {
        HudState::Recognizing => ProcessingMotion::recognizing(1.5, false),
        HudState::Polishing => ProcessingMotion::polishing(Fill::at_rest(carry.unwrap_or(0.5)), 1.5, 0., false),
        _ => return None,
    };
    Some(motion.at(t).value)
}

/// Success starts after the fill settles; the background is already becoming green/gold.
pub(crate) const OUTCOME_ICON_DELAY: f32 = SETTLE_S + 0.08;
pub(super) const OUTCOME_ICON_S: f32 = 0.32;
/// After the icon finishes drawing, the whole capsule (icon and background together) holds
/// briefly, then fades as one — neither outlives the other.
pub(super) const OUTCOME_HOLD_S: f32 = 0.24;
pub(super) const OUTCOME_HOLD_END: f32 = OUTCOME_ICON_DELAY + OUTCOME_ICON_S + OUTCOME_HOLD_S;
pub(crate) const OUTCOME_FADE_S: f32 = 0.26;
pub(crate) const OUTCOME_END: f32 = OUTCOME_HOLD_END + OUTCOME_FADE_S;

/// Copy times are relative to the icon clock, which begins after the progress fill settles.
pub(crate) const COPY_ENTER_AT: f32 = 0.35;
const COPY_STAGGER_S: f32 = 0.035;
const COPY_GLYPH_FADE_S: f32 = 0.22;
const COPY_SETTLE_AT: f32 = 0.9;
pub(crate) const COPY_HOLD_END: f32 = OUTCOME_ICON_DELAY + 1.45;
pub(crate) const COPY_FADE_S: f32 = 0.20;
pub(crate) const COPY_END: f32 = COPY_HOLD_END + COPY_FADE_S;

pub(super) fn copied_progress(t: Option<f32>, motion: Motion) -> f32 {
    let Some(t) = t.filter(|_| !motion.is_reduced()) else {
        return 1.;
    };
    if t >= COPY_SETTLE_AT {
        return 1.;
    }
    motion::spring_progress(t - COPY_ENTER_AT, 0.55, 18.)
}

/// Premeasure the entire label so the shell never expands in character-sized steps.
pub(super) fn copied_label_width(lang: Lang) -> f32 {
    ui::text_width(lang.tr(Tr::HudCopied), 12., FontWeight::NORMAL)
}

pub(crate) fn copied_growth(t: Option<f32>, motion: Motion, lang: Lang) -> f32 {
    let target = (COPY_ICON + 4. + copied_label_width(lang) + 38.).max(CAPSULE_W);
    (target - CAPSULE_W) * copied_progress(t, motion)
}

pub(super) fn copied_glyph(t: Option<f32>, index: usize, motion: Motion) -> (f32, f32) {
    let Some(t) = t else {
        return (1., 0.);
    };
    if motion.is_reduced() {
        return ((t / 0.1).clamp(0., 1.), 0.);
    }
    let local = t - COPY_ENTER_AT - index as f32 * COPY_STAGGER_S;
    let fade = (local / COPY_GLYPH_FADE_S).clamp(0., 1.);
    let rise = if local >= 0.4 { 1. } else { motion::spring_progress(local, 0.55, 24.) };
    (1. - (1. - fade).powi(3), 6. * (1. - rise))
}
