//! Time-derived state of the HUD view: the animation clock, the capsule width, fill progress and countdown.

use std::time::Instant;

use dito_core::controller::{HudState, MAX_RECORDING_DURATION};

use crate::hud::capsule::{self, WIDEN_S};
use crate::hud::motion::Spring;
use crate::hud::timing::{self, COPY_END, OUTCOME_END, OUTCOME_ICON_DELAY};
use crate::ui::Motion;

use super::{HudView, LABEL_FADE_S, POP_MS};

/// The recording countdown appears this many seconds into a recording, fading in over [`COUNTDOWN_FADE_S`].
pub(super) const COUNTDOWN_START_S: f32 = 240.;
pub(super) const COUNTDOWN_FADE_S: f32 = 0.18;

impl HudView {
    /// Advance the waveform, the error reason and the countdown by one tick; whether a frame is needed.
    pub(super) fn advance_clock(&mut self) -> bool {
        if matches!(self.state, HudState::Listening { .. }) {
            let now = Instant::now();
            let dt = now.duration_since(self.wave_tick).as_secs_f32();
            self.wave_tick = now;
            let level = self.st.recorder.level();
            #[cfg(feature = "screenshots")]
            let level = self
                .wave_preview_gain
                .map_or(level, |gain| crate::hud::waveform::speech_sample(self.opened.elapsed().as_secs_f32(), gain));
            self.waveform.advance(level, dt);
        }
        let error_target = f32::from(self.hover_btn.is_some());
        let in_error = matches!(self.state, HudState::Error { .. });
        let error_moving = in_error && self.error_reason.moving(error_target);
        let now = Instant::now();
        let dt = now.duration_since(self.error_tick).as_secs_f32();
        self.error_tick = now;
        let motion = Motion::system();
        if in_error {
            if motion.is_reduced() {
                self.error_reason = Spring { position: error_target, velocity: 0. };
            } else {
                self.error_reason.advance(error_target, dt, 0.72, 22.);
            }
        }
        let moving = matches!(self.state, HudState::Listening { .. } | HudState::Recognizing | HudState::Polishing);
        let entry = self.opened.elapsed().as_millis() < POP_MS as u128;
        let countdown = self.remaining();
        let countdown_changed = countdown != self.countdown_tick;
        self.countdown_tick = countdown;
        let countdown_entering = countdown.is_some()
            && self.recording_started.is_some_and(|s| {
                (COUNTDOWN_START_S..COUNTDOWN_START_S + COUNTDOWN_FADE_S).contains(&s.elapsed().as_secs_f32())
            });
        (moving && (!motion.is_reduced() || self.elapsed() < 0.30))
            || entry
            || self.widening()
            || self.completing()
            || error_moving
            || countdown_changed
            || countdown_entering
            || (self.countdown_exit.is_some() && self.elapsed() < LABEL_FADE_S)
    }

    pub(super) fn elapsed(&self) -> f32 {
        self.freeze.unwrap_or_else(|| self.since.elapsed().as_secs_f32())
    }

    /// Capsule width in design points: eases from where it was when the state changed to the state's own.
    /// Copied uses one continuous spring toward the full localized label width.
    pub(super) fn width(&self) -> f32 {
        let mut w = if Motion::system().is_reduced() {
            capsule::rest_width(&self.state)
        } else {
            capsule::eased_width(self.width_from, capsule::rest_width(&self.state), self.elapsed())
        };
        if matches!(self.state, HudState::Copied) {
            let t = (self.elapsed() - OUTCOME_ICON_DELAY).max(0.);
            let clock = self.freeze.map_or(Some(t), |f| (f > 0.).then_some(f));
            w += timing::copied_growth(clock, Motion::system(), self.st.cfg.get().general.lang());
        }
        w
    }

    pub(super) fn remaining(&self) -> Option<u64> {
        if !matches!(self.state, HudState::Listening { .. }) {
            return None;
        }
        let elapsed = self.freeze.unwrap_or_else(|| self.recording_started.map_or(0., |t| t.elapsed().as_secs_f32()));
        (elapsed >= COUNTDOWN_START_S)
            .then(|| ((MAX_RECORDING_DURATION.as_secs_f32() - elapsed).max(0.) as f64).ceil() as u64)
    }

    /// The capsule is still easing toward its width.
    fn widening(&self) -> bool {
        self.freeze.is_none() && self.since.elapsed().as_secs_f32() < WIDEN_S
    }

    fn completing(&self) -> bool {
        let end = match self.state {
            HudState::Copied => COPY_END,
            HudState::Inserted => OUTCOME_END,
            _ => return false,
        };
        self.freeze.is_none() && self.elapsed() < end
    }

    pub(super) fn progress(&self) -> Option<f32> {
        if matches!(self.state, HudState::Inserted | HudState::Copied) {
            return self.carry.map(|from| timing::settle(from, 1., self.elapsed()));
        }
        timing::motion_fill_at(&self.state, self.elapsed(), self.carry, Motion::system())
    }
}
