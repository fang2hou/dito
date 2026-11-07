//! Window-relative hit testing: which part of the HUD the pointer is on.

use dito_core::config::RecordMode;
use dito_core::controller::{ErrorAction, HudState};

use crate::hud::buttons::BTN;
use crate::hud::capsule::{CAPSULE_H, CAPSULE_PAD, CAPSULE_W};
use crate::hud::window::{WIN_PAD, WIN_W};

use super::language_chip::CHIP_H;
use super::layout::{CAPSULE_TOP, ERROR_BUBBLE_H, ZONE_GAP};

const UPPER_HALF_W: f32 = 90.;
const UPPER_H: f32 = 38.;
const ERROR_NOTE_HALF_W: f32 = 135.;
const ERROR_NOTE_BELOW_SLACK: f32 = 4.;
const STOP_HALF_W: f32 = 31.;
const STOP_TOP: f32 = 35.;

/// What the pointer is over.
pub(super) struct Hit {
    /// The hover zone: the capsule, the controls above it and the error reason.
    pub(super) in_zone: bool,
    /// The recording button under the pointer, by element id.
    pub(super) button: Option<&'static str>,
    /// Clicks here must reach the HUD instead of passing through the transparent window.
    pub(super) interactive: bool,
}

pub(super) fn pointer_hit(state: &HudState, width: f32, chip_width: f32, hover_zone: bool, x: f32, y: f32) -> Hit {
    let center = (WIN_W + 2. * WIN_PAD) / 2.;
    let top = CAPSULE_TOP;
    let left = center - width / 2.;
    let on_capsule = x >= left && x <= left + width && y >= top && y <= top + CAPSULE_H;
    let toggle = matches!(state, HudState::Listening { mode: RecordMode::Toggle });
    let error = matches!(state, HudState::Error { .. });
    let processing = matches!(state, HudState::Recognizing | HudState::Polishing);
    let inside_circle = |cx: f32| (x - cx).powi(2) + (y - (top + CAPSULE_H / 2.)).powi(2) <= (BTN / 2.).powi(2);
    let left_button = (toggle || error) && inside_circle(center - CAPSULE_W / 2. + CAPSULE_PAD + BTN / 2.);
    let right_button = (toggle || error) && inside_circle(center + CAPSULE_W / 2. - CAPSULE_PAD - BTN / 2.);
    let in_upper = x >= center - UPPER_HALF_W && x <= center + UPPER_HALF_W && y >= top - UPPER_H && y < top;
    let in_error_note = error
        && x >= center - ERROR_NOTE_HALF_W
        && x <= center + ERROR_NOTE_HALF_W
        && y >= top - ERROR_BUBBLE_H - ZONE_GAP
        && y <= top + CAPSULE_H + ZONE_GAP + ERROR_BUBBLE_H + ERROR_NOTE_BELOW_SLACK;
    let in_zone = on_capsule || in_error_note || ((toggle || processing) && hover_zone && in_upper);
    let button = hovered_button(state, left_button, right_button);
    let chip = toggle
        && hover_zone
        && button.is_none()
        && chip_width > 0.
        && (x - center).abs() <= chip_width / 2.
        && y >= top - ZONE_GAP - CHIP_H
        && y <= top - ZONE_GAP;
    let stop = processing
        && hover_zone
        && x >= center - STOP_HALF_W
        && x <= center + STOP_HALF_W
        && y >= top - STOP_TOP
        && y <= top - ZONE_GAP;
    Hit { in_zone, button, interactive: left_button || right_button || chip || stop }
}

fn hovered_button(state: &HudState, left: bool, right: bool) -> Option<&'static str> {
    match state {
        HudState::Listening { mode: RecordMode::Toggle } if left => Some("hud-cancel"),
        HudState::Listening { mode: RecordMode::Toggle } if right => Some("hud-finish"),
        HudState::Error { .. } if left => Some("hud-dismiss"),
        HudState::Error { action: ErrorAction::Retry, .. } if right => Some("hud-retry"),
        HudState::Error { action: ErrorAction::OpenSettings, .. } if right => Some("hud-settings"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use dito_core::config::RecordMode;
    use dito_core::controller::{ErrorAction, HudState};

    use super::{BTN, CAPSULE_PAD, CAPSULE_TOP, CAPSULE_W, WIN_PAD, WIN_W, pointer_hit};

    #[test]
    fn the_lower_error_reason_pauses_dismissal_but_does_not_intercept_clicks() {
        let error = HudState::Error {
            kind: dito_core::error::ErrorKind::Mic,
            detail: String::new(),
            action: ErrorAction::OpenSettings,
        };
        let center = (WIN_W + 2. * WIN_PAD) / 2.;
        let hit = pointer_hit(&error, CAPSULE_W, 143.5, true, center, CAPSULE_TOP + 55.);
        assert!(hit.in_zone);
        assert!(hit.button.is_none());
        assert!(!hit.interactive);
        assert!(!pointer_hit(&error, CAPSULE_W, 143.5, true, center, CAPSULE_TOP + 80.).in_zone);
    }

    #[test]
    fn the_language_hit_area_follows_the_configured_width() {
        let toggle = HudState::Listening { mode: RecordMode::Toggle };
        let center = (WIN_W + 2. * WIN_PAD) / 2.;
        let y = CAPSULE_TOP - 20.;
        assert!(!pointer_hit(&toggle, CAPSULE_W, 0., true, center, y).interactive);
        assert!(pointer_hit(&toggle, CAPSULE_W, 75.5, true, center, y).interactive);
        assert!(!pointer_hit(&toggle, CAPSULE_W, 75.5, true, center + 50., y).interactive);
    }

    #[test]
    fn only_controls_intercept_clicks() {
        let toggle = HudState::Listening { mode: RecordMode::Toggle };
        let hold = HudState::Listening { mode: RecordMode::Hold };
        let center = (WIN_W + 2. * WIN_PAD) / 2.;
        let top = CAPSULE_TOP;
        let button_x = center - CAPSULE_W / 2. + CAPSULE_PAD + BTN / 2.;
        let interactive =
            |state: &HudState, width, zone, x, y| pointer_hit(state, width, 143.5, zone, x, y).interactive;
        assert!(interactive(&toggle, CAPSULE_W, false, button_x, top + 16.5));
        assert!(!interactive(&toggle, CAPSULE_W, false, center, top + 16.5));
        assert!(!interactive(&hold, 72., false, button_x, top + 16.5));
        assert!(!interactive(&toggle, CAPSULE_W, true, center, top + 50.));
        assert!(interactive(&toggle, CAPSULE_W, true, center, top - 20.));
        assert!(interactive(&HudState::Recognizing, CAPSULE_W, true, center, top - 20.));
        assert!(!interactive(&HudState::Recognizing, CAPSULE_W, true, center + 80., top - 20.));
    }
}
