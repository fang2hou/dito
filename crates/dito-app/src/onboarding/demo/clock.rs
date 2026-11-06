use std::time::Instant;

use dito_core::config::RecordMode;

use crate::ui::Motion;

use super::timeline::{PERIOD, STILL_AT, SWITCH_S, smooth};

/// One picture of the stage: `mode`'s loop `at` seconds in, drawn with `opacity`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layer {
    pub(super) mode: RecordMode,
    pub(super) at: f32,
    pub(super) opacity: f32,
}

/// What to draw this frame: the pictures to stack (the old one first, while a mode switch cross-fades), whether this
/// is the still of Reduce Motion (the waveform rests and a held key keeps its size, like the design's static frame),
/// and whether another frame is due.
pub(super) struct Tick {
    pub(super) layers: Vec<Layer>,
    pub(super) still: bool,
    pub(super) running: bool,
}

impl Tick {
    /// Keyframe 2, nothing moving.
    pub(super) fn still(mode: RecordMode) -> Self {
        Self { layers: vec![Layer { mode, at: STILL_AT, opacity: 1. }], still: true, running: false }
    }
}

/// An outgoing mode's last picture, fading out.
pub(super) struct Switch {
    mode: RecordMode,
    at: f32,
    since: Instant,
}

/// The loop's clock. gpui keeps it with the element across frames.
pub(super) struct Clock {
    mode: RecordMode,
    /// Seconds into the loop. It only advances while the window is active, so the loop resumes where it paused.
    at: f32,
    /// When `at` last advanced; `None` while paused.
    last: Option<Instant>,
    switch: Option<Switch>,
}

impl Clock {
    pub(super) fn new(mode: RecordMode) -> Self {
        Self { mode, at: 0., last: None, switch: None }
    }

    pub(super) fn tick(&mut self, mode: RecordMode, now: Instant, active: bool, motion: Motion) -> Tick {
        if motion.is_reduced() {
            *self = Self::new(mode);
            return Tick::still(mode);
        }
        if mode != self.mode {
            // The old picture stays as it was and fades out, unless nobody is looking.
            let switch = active.then_some(Switch { mode: self.mode, at: self.at, since: now });
            *self = Self { switch, ..Self::new(mode) };
        }
        if active {
            if let Some(last) = self.last {
                self.at = (self.at + now.saturating_duration_since(last).as_secs_f32()) % PERIOD;
            }
            self.last = Some(now);
        } else {
            self.last = None;
        }
        let mut layers = vec![Layer { mode: self.mode, at: self.at, opacity: 1. }];
        if let Some(sw) = &self.switch {
            let u = now.saturating_duration_since(sw.since).as_secs_f32() / SWITCH_S;
            if u < 1. {
                let u = smooth(u);
                layers[0].opacity = u;
                layers.insert(0, Layer { mode: sw.mode, at: sw.at, opacity: 1. - u });
            } else {
                self.switch = None;
            }
        }
        Tick { layers, still: false, running: active }
    }
}

/// Screenshots: how a frozen demo is held.
#[cfg(feature = "screenshots")]
#[derive(Clone, Copy)]
pub enum Frozen {
    /// This many seconds into the loop.
    At(f32),
    /// The still Reduce Motion shows.
    Still,
}

#[cfg(feature = "screenshots")]
thread_local! {
    static FROZEN: std::cell::Cell<Option<Frozen>> = const { std::cell::Cell::new(None) };
}

/// Screenshots: hold every demo as `frozen` says (`None` lets them run again).
#[cfg(feature = "screenshots")]
pub fn freeze(frozen: Option<Frozen>) {
    FROZEN.with(|f| f.set(frozen));
}

#[cfg(feature = "screenshots")]
pub(super) fn frozen(mode: RecordMode) -> Option<Tick> {
    FROZEN.with(|f| f.get()).map(|frozen| match frozen {
        Frozen::At(at) => Tick { layers: vec![Layer { mode, at, opacity: 1. }], still: false, running: false },
        Frozen::Still => Tick::still(mode),
    })
}

#[cfg(not(feature = "screenshots"))]
pub(super) fn frozen(_mode: RecordMode) -> Option<Tick> {
    None
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use dito_core::config::RecordMode;
    use dito_core::controller::HudState;

    use super::{Clock, Layer, Motion, PERIOD, STILL_AT};
    use crate::hud;
    use crate::onboarding::demo::timeline::Scene;

    fn after(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn the_clock_runs_loops_and_pauses_while_inactive() {
        let base = Instant::now();
        let mut clock = Clock::new(RecordMode::Toggle);
        let first = clock.tick(RecordMode::Toggle, base, true, Motion::Full);
        assert_eq!(first.layers, vec![Layer { mode: RecordMode::Toggle, at: 0., opacity: 1. }]);
        assert!(first.running && !first.still);
        let t = clock.tick(RecordMode::Toggle, after(base, 1500), true, Motion::Full);
        assert!((t.layers[0].at - 1.5).abs() < 1e-3);
        // The window deactivates: the loop stops, and no more frames are asked for.
        let paused = clock.tick(RecordMode::Toggle, after(base, 2000), false, Motion::Full);
        assert!(!paused.running);
        let still = clock.tick(RecordMode::Toggle, after(base, 60_000), false, Motion::Full);
        assert!((still.layers[0].at - 1.5).abs() < 1e-3, "frozen while inactive: {}", still.layers[0].at);
        // It comes back a minute later and carries on from the same phase.
        let back = clock.tick(RecordMode::Toggle, after(base, 60_100), true, Motion::Full);
        assert!((back.layers[0].at - 1.5).abs() < 1e-3, "resumes where it paused: {}", back.layers[0].at);
        let go = clock.tick(RecordMode::Toggle, after(base, 60_600), true, Motion::Full);
        assert!((go.layers[0].at - 2.0).abs() < 1e-3);
        // And it wraps at the end of the loop.
        let wrapped = clock.tick(RecordMode::Toggle, after(base, 60_600 + 4_000), true, Motion::Full);
        assert!((wrapped.layers[0].at - (2.0 + 4.0 - PERIOD)).abs() < 1e-2, "{}", wrapped.layers[0].at);
    }

    #[test]
    fn switching_modes_cross_fades_the_old_picture_into_a_new_loop_from_zero() {
        let base = Instant::now();
        let mut clock = Clock::new(RecordMode::Toggle);
        clock.tick(RecordMode::Toggle, base, true, Motion::Full);
        clock.tick(RecordMode::Toggle, after(base, 2000), true, Motion::Full);
        let start = clock.tick(RecordMode::Hold, after(base, 2000), true, Motion::Full);
        assert_eq!(start.layers.len(), 2, "old picture lies under the new one");
        let (old, new) = (start.layers[0], start.layers[1]);
        assert_eq!((old.mode, new.mode), (RecordMode::Toggle, RecordMode::Hold));
        assert!((old.at - 2.0).abs() < 1e-3, "the old picture holds the moment it was switched away from");
        assert_eq!((old.opacity, new.opacity, new.at), (1., 0., 0.));
        let mid = clock.tick(RecordMode::Hold, after(base, 2100), true, Motion::Full);
        assert!((mid.layers[0].opacity - 0.5).abs() < 1e-3 && (mid.layers[1].opacity - 0.5).abs() < 1e-3);
        assert!((mid.layers[1].at - 0.1).abs() < 1e-3, "the new loop already runs");
        let done = clock.tick(RecordMode::Hold, after(base, 2250), true, Motion::Full);
        assert_eq!(done.layers.len(), 1);
        assert_eq!(done.layers[0].opacity, 1.);
        assert!((done.layers[0].at - 0.25).abs() < 1e-3);
    }

    #[test]
    fn reduce_motion_holds_the_recording_keyframe_without_a_loop() {
        let base = Instant::now();
        let mut clock = Clock::new(RecordMode::Hold);
        for ms in [0, 700, 5_000] {
            let t = clock.tick(RecordMode::Hold, after(base, ms), true, Motion::Reduced);
            assert_eq!(t.layers, vec![Layer { mode: RecordMode::Hold, at: STILL_AT, opacity: 1. }]);
            assert!(!t.running && t.still);
        }
        // A mode switch is just a new still, not a fade.
        let t = clock.tick(RecordMode::Toggle, after(base, 6_000), true, Motion::Reduced);
        assert_eq!(t.layers.len(), 1);
        // The keyframe is the recording: key per mode, capsule listening, chip 2 lit.
        let hold = Scene::at(RecordMode::Hold, STILL_AT);
        assert_eq!(hold.key, 1.);
        assert_eq!(hold.hud.as_ref().map(|h| h.w), Some(hud::NARROW_W));
        let toggle = Scene::at(RecordMode::Toggle, STILL_AT);
        assert_eq!(toggle.key, 0.);
        assert_eq!(toggle.hud.as_ref().map(|h| h.w), Some(hud::CAPSULE_W));
        for scene in [hold, toggle] {
            assert!(matches!(scene.hud.as_ref().map(|h| &h.state), Some(HudState::Listening { .. })));
            assert!((scene.chips[1] - 1.).abs() < 1e-6 && scene.chips[0] < 1e-6 && scene.chips[2] < 1e-6);
        }
    }
}
