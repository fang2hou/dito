//! How far the notes panel has come: one slide and one fade, each retargetable from where it is, and the width rule
//! that decides between side by side and taking turns.

use std::time::Instant;

use super::{GAP, PANEL_W};
use crate::tools::transcription::morph::{Ease, Fade, Timing};
use crate::ui::Motion;

/// The panel slides in and fades up over this long, on an ease-out cubic.
const SLIDE: Timing = Timing { duration_ms: 240., delay_ms: 0., curve: Ease::Out };
/// With Reduce Motion the widths change at once and only the fade runs.
const SHORT_FADE: Timing = Timing { duration_ms: 120., delay_ms: 0., curve: Ease::InOut };

/// The content width below which transcript and notes take turns instead of sitting side by side: the transcript
/// keeps 420 pt beside the panel. The way back needs 24 pt more, so a drag across the edge does not flicker.
const SIDE_BY_SIDE: f32 = 420. + GAP + PANEL_W;
const SIDE_BY_SIDE_BACK: f32 = SIDE_BY_SIDE + 24.;

pub(super) struct Reveal {
    slide: Fade,
    fade: Fade,
}

impl Reveal {
    pub fn new() -> Self {
        Self { slide: Fade::new(0.), fade: Fade::new(0.) }
    }

    /// Aims the motion at the open or the closed panel. A change in flight turns around from where it is, so a quick
    /// second press never jumps.
    pub fn retarget(&mut self, open: bool, motion: Motion, now: Instant) {
        let target = f32::from(open);
        if motion.is_reduced() {
            let changed = self.slide.target() != target;
            self.slide.retarget(target, SLIDE, now);
            if changed {
                self.slide.settle();
            }
            self.fade.retarget(target, SHORT_FADE, now);
        } else {
            self.slide.retarget(target, SLIDE, now);
            self.fade.retarget(target, SLIDE, now);
        }
    }

    /// The slide, for the widths, and the fade, for the opacity.
    pub fn sample(&self, now: Instant) -> (f32, f32) {
        (self.slide.sample(now), self.fade.sample(now))
    }

    pub fn settled(&self, now: Instant) -> bool {
        self.slide.settled(now) && self.fade.settled(now)
    }

    /// Goes to the closed pose without motion, for a panel that closes because its transcript went away.
    pub fn close_now(&mut self, now: Instant) {
        self.slide.retarget(0., SLIDE, now);
        self.fade.retarget(0., SLIDE, now);
        self.slide.settle();
        self.fade.settle();
    }
}

/// Side by side while the transcript keeps 420 pt beside the panel, taking turns below that.
pub(super) fn compact_for(width: f32, was_compact: bool) -> bool {
    if was_compact { width < SIDE_BY_SIDE_BACK } else { width < SIDE_BY_SIDE }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn at(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn the_panel_comes_in_over_240_ms_on_an_ease_out_cubic() {
        let start = Instant::now();
        let mut reveal = Reveal::new();
        reveal.retarget(true, Motion::Full, start);
        let (slide, fade) = reveal.sample(at(start, 0));
        assert_eq!((slide, fade), (0., 0.));
        let (half, _) = reveal.sample(at(start, 50));
        assert!((half - (1. - (1. - 50. / 240.0_f32).powi(3))).abs() < 1e-4, "ease-out cubic: {half}");
        assert!(half > 0.45, "most of the move comes early");
        assert!(!reveal.settled(at(start, 239)));
        assert_eq!(reveal.sample(at(start, 240)), (1., 1.));
        assert!(reveal.settled(at(start, 240)));
    }

    #[test]
    fn a_second_press_in_flight_turns_around_from_where_the_panel_is() {
        let start = Instant::now();
        let mut reveal = Reveal::new();
        reveal.retarget(true, Motion::Full, start);
        let (mid, _) = reveal.sample(at(start, 60));
        reveal.retarget(false, Motion::Full, at(start, 60));
        let (now, _) = reveal.sample(at(start, 60));
        assert!((now - mid).abs() < 1e-4, "no jump at the turn: {mid} then {now}");
        let (later, _) = reveal.sample(at(start, 120));
        assert!(later < mid, "it heads back");
        assert_eq!(reveal.sample(at(start, 60 + 240)), (0., 0.));
        reveal.retarget(true, Motion::Full, at(start, 400));
        reveal.retarget(false, Motion::Full, at(start, 400));
        assert_eq!(reveal.sample(at(start, 400)), (0., 0.), "pressing and releasing within a frame moves nothing");
    }

    #[test]
    fn reduce_motion_applies_the_width_at_once_and_only_fades() {
        let start = Instant::now();
        let mut reveal = Reveal::new();
        reveal.retarget(true, Motion::Reduced, start);
        let (slide, fade) = reveal.sample(at(start, 0));
        assert_eq!(slide, 1., "the final width is applied immediately");
        assert_eq!(fade, 0., "the fade starts from nothing");
        assert!(reveal.sample(at(start, 60)).1 > 0. && reveal.sample(at(start, 60)).1 < 1.);
        assert_eq!(reveal.sample(at(start, 120)).1, 1.);
        reveal.retarget(false, Motion::Reduced, at(start, 300));
        assert_eq!(reveal.sample(at(start, 300)).0, 0.);
    }

    #[test]
    fn closing_for_a_vanished_transcript_does_not_animate() {
        let start = Instant::now();
        let mut reveal = Reveal::new();
        reveal.retarget(true, Motion::Full, start);
        reveal.close_now(at(start, 10));
        assert_eq!(reveal.sample(at(start, 10)), (0., 0.));
        assert!(reveal.settled(at(start, 10)));
    }

    #[test]
    fn notes_and_transcript_take_turns_below_776_pt_with_a_hysteresis() {
        assert!(!compact_for(776., false), "the design's width fits both");
        assert!(compact_for(775., false));
        assert!(compact_for(790., true), "once compact, it needs 24 pt more");
        assert!(!compact_for(800., true));
    }
}
