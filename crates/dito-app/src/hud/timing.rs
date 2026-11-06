//! The HUD's clocks: progress fill, outcome hold and fade, and the copied label's growth.

use dito_core::controller::HudState;
use dito_core::i18n::{Lang, Tr};
use gpui_kit::*;

use crate::ui::{self, Motion};

use super::capsule::CAPSULE_W;
use super::motion;
use super::outcome_icon::COPY_ICON;

pub(super) const SETTLE_S: f32 = 0.18;

/// Ease toward the phase boundary, stopping 1.5 percentage points short until a real response.
/// A carried recognition fill settles to 50% before the polishing phase begins.
pub(crate) fn fill_at(state: &HudState, t: f32, carry: Option<f32>) -> Option<f32> {
    let (a, b) = match state {
        HudState::Recognizing => (0., 0.5),
        HudState::Polishing => (0.5, 1.),
        _ => return None,
    };
    let mut elapsed = t.max(0.);
    if let Some(from) = carry {
        if elapsed < SETTLE_S {
            return Some(settle(from, a, elapsed));
        }
        elapsed -= SETTLE_S;
    }
    let u = (elapsed / 1.5).clamp(0., 1.);
    Some(a + (b - a - 0.015) * (1. - (1. - u).powi(3)))
}

pub(super) fn settle(from: f32, to: f32, t: f32) -> f32 {
    let u = (t / SETTLE_S).clamp(0., 1.);
    from + (to - from) * ui::spring(u, Motion::Full)
}

pub(super) fn motion_fill_at(state: &HudState, t: f32, carry: Option<f32>, motion: Motion) -> Option<f32> {
    if !motion.is_reduced() {
        return fill_at(state, t, carry);
    }
    let (start, target) = match state {
        HudState::Recognizing => (0., 0.485),
        HudState::Polishing => (0.5, 0.985),
        _ => return None,
    };
    let u = (t / 0.12).clamp(0., 1.);
    Some(carry.unwrap_or(start) + (target - carry.unwrap_or(start)) * (1. - (1. - u).powi(3)))
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

#[cfg(test)]
mod tests {
    use dito_core::config::RecordMode;

    use super::{Motion, SETTLE_S, fill_at, motion_fill_at, settle};
    use dito_core::controller::HudState;

    #[test]
    fn the_fill_runs_half_then_half_and_lands_an_early_finish() {
        assert_eq!(fill_at(&HudState::Listening { mode: RecordMode::Hold }, 1., None), None);
        assert_eq!(fill_at(&HudState::Inserted, 1., None), None);
        let recognizing = |t| fill_at(&HudState::Recognizing, t, None).unwrap();
        assert_eq!(recognizing(0.), 0.);
        assert!(recognizing(0.75) > 0.25);
        assert!((recognizing(1.5) - 0.485).abs() < 1e-6);
        assert_eq!(recognizing(1.5), recognizing(60.), "hold without creeping or claiming completion");
        let polishing = |t, carry| fill_at(&HudState::Polishing, t, carry).unwrap();
        assert_eq!(polishing(0., None), 0.5);
        assert!((polishing(1.5, None) - 0.985).abs() < 1e-6);
        assert_eq!(polishing(60., None), polishing(1.5, None));
        assert!((polishing(0., Some(0.3)) - 0.3).abs() < 1e-6);
        assert!((polishing(SETTLE_S, Some(0.3)) - 0.5).abs() < 1e-6);
        assert!((settle(0.62, 1., SETTLE_S) - 1.).abs() < 1e-6);
    }

    #[test]
    fn reduced_motion_settles_once_instead_of_continuously_advancing() {
        for (state, from, target) in [(HudState::Recognizing, 0., 0.485), (HudState::Polishing, 0.5, 0.985)] {
            assert_eq!(motion_fill_at(&state, 0., None, Motion::Reduced), Some(from));
            assert!((motion_fill_at(&state, 0.12, None, Motion::Reduced).unwrap() - target).abs() < 1e-6);
            assert_eq!(
                motion_fill_at(&state, 0.12, None, Motion::Reduced),
                motion_fill_at(&state, 10., None, Motion::Reduced)
            );
        }
    }
}
