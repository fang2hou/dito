//! When a refresh of the notes is due. Pure: the runner feeds it the clock and what it sees, and the rules are
//! tested without a model.

use std::time::Duration;

/// While text keeps arriving, the notes refresh no more often than this.
pub const REFRESH_EVERY: Duration = Duration::from_secs(60);

/// Once the recording is over, the last refresh waits this long after the one before it.
const FINAL_GAP: Duration = Duration::from_secs(5);

/// What the runner sees at one look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Poll {
    /// A monotonic clock.
    pub now: Duration,
    /// Changes whenever the transcript reaches further or holds more.
    pub fingerprint: u64,
    pub has_text: bool,
    pub paused: bool,
    /// The session is over: no more text will arrive, so the last refresh may run ahead of the interval.
    pub ending: bool,
}

#[derive(Debug)]
pub struct Scheduler {
    every: Duration,
    last_start: Option<Duration>,
    last_ok: Option<u64>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new(REFRESH_EVERY)
    }
}

impl Scheduler {
    pub fn new(every: Duration) -> Self {
        Self { every, last_start: None, last_ok: None }
    }

    /// Whether a refresh should start now. At most one runs at a time: the caller asks again only when none is
    /// running.
    pub fn due(&self, poll: &Poll) -> bool {
        if !poll.has_text || self.last_ok == Some(poll.fingerprint) {
            return false;
        }
        let Some(last) = self.last_start else { return true };
        let since = poll.now.saturating_sub(last);
        if poll.ending {
            return since >= self.every.min(FINAL_GAP);
        }
        !poll.paused && since >= self.every
    }

    /// The refresh that succeeded last covered the transcript as `fingerprint` saw it.
    pub fn covers(&self, fingerprint: u64) -> bool {
        self.last_ok == Some(fingerprint)
    }

    pub fn started(&mut self, now: Duration) {
        self.last_start = Some(now);
    }

    /// The refresh covered the transcript as `fingerprint` saw it.
    pub fn succeeded(&mut self, fingerprint: u64) {
        self.last_ok = Some(fingerprint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poll(secs: u64, fingerprint: u64) -> Poll {
        Poll { now: Duration::from_secs(secs), fingerprint, has_text: true, paused: false, ending: false }
    }

    #[test]
    fn the_first_look_with_text_starts_at_once_and_an_empty_transcript_waits() {
        let s = Scheduler::default();
        assert!(s.due(&poll(0, 1)));
        assert!(!s.due(&Poll { has_text: false, ..poll(0, 0) }));
    }

    #[test]
    fn new_text_refreshes_about_once_a_minute_and_the_same_text_never_does() {
        let mut s = Scheduler::default();
        s.started(Duration::from_secs(0));
        s.succeeded(1);
        assert!(!s.due(&poll(30, 2)), "too soon");
        assert!(!s.due(&poll(61, 1)), "nothing new");
        assert!(s.due(&poll(60, 2)));
        s.started(Duration::from_secs(60));
        s.succeeded(2);
        assert!(!s.due(&poll(119, 3)));
        assert!(s.due(&poll(125, 3)));
    }

    #[test]
    fn a_failed_refresh_is_tried_again_after_the_interval_not_on_every_look() {
        let mut s = Scheduler::default();
        s.started(Duration::from_secs(0));
        assert!(!s.due(&poll(10, 1)));
        assert!(s.due(&poll(60, 1)));
    }

    #[test]
    fn a_pause_holds_refreshes_and_the_end_ignores_the_interval_once() {
        let mut s = Scheduler::default();
        s.started(Duration::from_secs(0));
        s.succeeded(1);
        assert!(!s.due(&Poll { paused: true, ..poll(600, 2) }));
        assert!(s.due(&Poll { ending: true, ..poll(10, 2) }));
        s.started(Duration::from_secs(10));
        s.succeeded(2);
        assert!(!s.due(&Poll { ending: true, ..poll(20, 2) }), "the final text is covered");
    }

    #[test]
    fn a_paused_meeting_still_gets_its_first_notes() {
        let s = Scheduler::default();
        assert!(s.due(&Poll { paused: true, ..poll(0, 1) }));
    }
}
