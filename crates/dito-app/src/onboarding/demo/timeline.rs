use dito_core::config::RecordMode;
use dito_core::controller::HudState;

use crate::hud;
use crate::ui::{self, Motion};

pub(super) const T_PRESS: f32 = 0.6;
/// Toggle: the second press. Hold: the release.
pub(super) const T_SECOND: f32 = 3.0;
pub(super) const T_INSERT: f32 = 4.8;
pub(super) const T_REST_END: f32 = T_INSERT + hud::OUTCOME_END;
pub(super) const RESTART_S: f32 = 0.2;
pub(super) const PERIOD: f32 = T_REST_END + RESTART_S;
/// The key leads the HUD by this much.
pub(super) const LEAD: f32 = 0.08;
pub(super) const KEY_DOWN_S: f32 = 0.12;
pub(super) const KEY_UP_S: f32 = 0.16;
/// A tap stays down this long once it has bottomed out.
pub(super) const TAP_HOLD_S: f32 = 0.10;
pub(super) const KEY_SCALE: f32 = 0.94;
pub(super) const INSERT_FADE_S: f32 = 0.16;
pub(super) const CHIP_FADE_S: f32 = 0.16;
pub(super) const SWITCH_S: f32 = 0.2;
/// Keyframe 2 (recording): the picture Reduce Motion holds.
pub(super) const STILL_AT: f32 = 2.0;
/// Where each phase chip's highlight begins; it ends where the next begins (the last, at the end of the rest).
pub(super) const CHIP_STARTS: [f32; 4] = [T_PRESS, 1.2, T_SECOND, T_INSERT];

pub(super) fn smooth(u: f32) -> f32 {
    ui::ease(u.clamp(0., 1.))
}

pub(super) fn ease_out(u: f32) -> f32 {
    1. - (1. - u.clamp(0., 1.)).powi(3)
}

pub(super) fn press(at: f32, down: f32, up: f32) -> f32 {
    if at < down {
        0.
    } else if at < up {
        ease_out((at - down) / KEY_DOWN_S)
    } else {
        (1. - ui::spring((at - up) / KEY_UP_S, Motion::Full)).clamp(0., 1.)
    }
}

pub(super) struct HudAt {
    pub(super) state: HudState,
    pub(super) since: f32,
    pub(super) w: f32,
    /// 0 to 1 through the pop-in while it is arriving.
    pub(super) pop: Option<f32>,
    /// Seconds since the green inserted capsule began, including its cross-fade.
    pub(super) inserted_elapsed: Option<f32>,
    /// 1 shown … 0 gone: the way back to the start fades it out.
    pub(super) shown: f32,
}

/// Continuous numbers, so the loop's cross-fade back to the start is just these running to their values at 0.
pub(super) struct Scene {
    /// 0 up … 1 down.
    pub(super) key: f32,
    pub(super) hud: Option<HudAt>,
    pub(super) chips: [f32; 4],
}

impl Scene {
    pub(super) fn at(mode: RecordMode, at: f32) -> Scene {
        let tap = KEY_DOWN_S + TAP_HOLD_S;
        let key = match mode {
            RecordMode::Hold => press(at, T_PRESS, T_SECOND),
            RecordMode::Toggle => press(at, T_PRESS, T_PRESS + tap).max(press(at, T_SECOND, T_SECOND + tap)),
        };
        let listen_at = T_PRESS + LEAD;
        let process_at = T_SECOND + LEAD;
        let listening = HudState::Listening { mode };
        let hud = (at >= listen_at).then(|| {
            let narrow = hud::rest_width(&listening);
            let (state, began, w) = if at < process_at {
                (listening, listen_at, narrow)
            } else {
                (HudState::Transcribing, process_at, hud::eased_width(narrow, hud::CAPSULE_W, at - process_at))
            };
            let pop = (at - listen_at) / (hud::POP_MS as f32 / 1000.);
            HudAt {
                state,
                since: at - began,
                w,
                pop: (pop < 1.).then_some(pop),
                inserted_elapsed: (at >= T_INSERT).then_some(at - T_INSERT),
                shown: 1. - smooth((at - T_REST_END) / RESTART_S),
            }
        });
        let mut chips = [0.; 4];
        for (i, chip) in chips.iter_mut().enumerate() {
            let fall = match CHIP_STARTS.get(i + 1) {
                Some(next) => smooth((at - next) / CHIP_FADE_S),
                None => smooth((at - T_REST_END) / RESTART_S),
            };
            *chip = smooth((at - CHIP_STARTS[i]) / CHIP_FADE_S) * (1. - fall);
        }
        Scene { key, hud, chips }
    }
}
