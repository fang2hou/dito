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
/// The completed contour receives one gentle light pulse, then the whole capsule exits.
pub(super) const OUTCOME_HOLD_END: f32 = OUTCOME_ICON_DELAY + 0.96;
pub(crate) const OUTCOME_FADE_S: f32 = 0.18;
pub(crate) const OUTCOME_END: f32 = OUTCOME_HOLD_END + OUTCOME_FADE_S;

pub(crate) const COPY_ENTER_AT: f32 = 0.24;
const COPY_STAGGER_S: f32 = 0.026;
const COPY_GLYPH_FADE_S: f32 = 0.028;
pub(crate) const COPY_HOLD_END: f32 = OUTCOME_ICON_DELAY + 1.45;
pub(crate) const COPY_FADE_S: f32 = 0.20;
pub(crate) const COPY_END: f32 = COPY_HOLD_END + COPY_FADE_S;
pub(super) const COPY_FONT: f32 = 11.;
pub(super) const COPY_GAP: f32 = 6.;

pub(super) fn check_halo(t: Option<f32>, motion: Motion) -> f32 {
    let Some(t) = t.filter(|_| !motion.is_reduced()) else {
        return 0.;
    };
    if t <= 0.32 {
        0.
    } else if t < 0.50 {
        motion::smooth((t - 0.32) / 0.18)
    } else if t < 0.58 {
        1.
    } else {
        1. - motion::smooth((t - 0.58) / 0.28)
    }
}

pub(crate) fn outcome_opacity(elapsed: f32, hold_end: f32, fade: f32, motion: Motion) -> f32 {
    if motion.is_reduced() {
        return f32::from(elapsed < hold_end + fade);
    }
    1. - motion::smooth((elapsed - hold_end) / fade)
}

pub(super) fn copied_progress(t: Option<f32>, motion: Motion) -> f32 {
    let Some(t) = t.filter(|_| !motion.is_reduced()) else {
        return 1.;
    };
    let peak = 1.9 / 21. + 1.;
    if t <= COPY_ENTER_AT {
        0.
    } else if t < 0.44 {
        peak * (1. - (1. - (t - COPY_ENTER_AT) / 0.20).powi(3))
    } else {
        peak + (1. - peak) * motion::smooth((t - 0.44) / 0.10)
    }
}

pub(super) fn copied_label_width(lang: Lang) -> f32 {
    ui::text_width(lang.tr(Tr::HudCopied), COPY_FONT, FontWeight::NORMAL)
}

pub(crate) fn copied_growth(t: Option<f32>, motion: Motion, lang: Lang) -> f32 {
    let target = (COPY_ICON + COPY_GAP + copied_label_width(lang) + 40.).max(CAPSULE_W);
    (target - CAPSULE_W) * copied_progress(t, motion)
}

pub(super) fn copied_sheets(t: Option<f32>, motion: Motion) -> (f32, f32, f32) {
    let Some(t) = t.filter(|_| !motion.is_reduced()) else {
        return (1., 1., COPY_ICON);
    };
    let ease = |u: f32| u.clamp(0., 1.);
    let front = ease(t / 0.12);
    let back = ease((t - 0.12) / 0.10);
    (front, back, 10.5 + (COPY_ICON - 10.5) * back)
}

/// Alpha, baseline displacement and active advance each use the character's own clock.
pub(super) fn copied_glyph(t: Option<f32>, index: usize, motion: Motion) -> (f32, f32, f32) {
    let Some(t) = t.filter(|_| !motion.is_reduced()) else {
        return (1., 0., 1.);
    };
    let local = t - COPY_ENTER_AT - index as f32 * COPY_STAGGER_S;
    // The storyboard reveals most of the glyph while it is still below the baseline.
    // Smoothstep hid that early travel and made the same 6 pt motion feel much smaller.
    let fade = (local / COPY_GLYPH_FADE_S).clamp(0., 1.);
    let alpha = 1. - (1. - fade).powi(2);
    let peak_time = std::f32::consts::PI / (43. * (1_f32 - 0.4 * 0.4).sqrt());
    let offset = if local <= 0. {
        6.
    } else if local < peak_time {
        6. * (1. - motion::spring_progress(local, 0.4, 43.))
    } else {
        let peak = -6. * (-0.4 * 43. * peak_time).exp();
        peak * (1. - motion::smooth((local - peak_time) / (0.165 - peak_time)))
    };
    (alpha, offset, alpha)
}
