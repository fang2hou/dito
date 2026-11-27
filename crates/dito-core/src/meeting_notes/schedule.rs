//! Which request to run next, given the recording and monotonic clocks.

use std::time::Duration;

pub(super) const HINT_EVERY: Duration = Duration::from_secs(30);
pub(super) const FINAL_GAP: Duration = Duration::from_secs(5);
const MEMO_EVERY_MS: u64 = 5 * 60 * 1_000;
const LATE_OPEN_MS: u64 = 2 * 60 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Job {
    Hint,
    Memo { last: bool },
}

#[derive(Clone, Copy)]
pub(super) enum Outcome {
    Succeeded { more: bool },
    Failed,
}

pub(super) struct Poll {
    pub now: Duration,
    pub meeting_ms: u64,
    pub covered_ms: u64,
    pub has_text: bool,
    pub hint_changed: bool,
    pub memo_changed: bool,
    pub paused: bool,
    pub ending: bool,
}

pub(super) struct Scheduler {
    hint_every: Duration,
    final_gap: Duration,
    hint: Attempts,
    memo: Attempts,
    hint_attempted: bool,
    late_open: bool,
    memo_started_ms: Option<u64>,
    memo_more: bool,
    final_started: bool,
    last_start: Option<Duration>,
}

#[derive(Default)]
struct Attempts {
    started: Option<Duration>,
    took: Duration,
    retry_at: Duration,
    failures: usize,
}

impl Attempts {
    fn ready(&self, now: Duration) -> bool {
        now >= self.retry_at
    }

    fn finished(&mut self, now: Duration, success: bool, backoff_unit: Duration) {
        self.took = now.saturating_sub(self.started.unwrap_or(now));
        if success {
            self.failures = 0;
            self.retry_at = Duration::ZERO;
        } else {
            let units = [1, 2, 4, 8, 10];
            self.retry_at = now + backoff_unit * units[self.failures.min(units.len() - 1)];
            self.failures = self.failures.saturating_add(1);
        }
    }
}

impl Scheduler {
    pub fn new(hint_every: Duration, final_gap: Duration, seeded_hint: bool, memo_ms: Option<u64>) -> Self {
        Self {
            hint_every,
            final_gap,
            hint: Attempts::default(),
            memo: Attempts::default(),
            hint_attempted: seeded_hint || memo_ms.is_some(),
            late_open: false,
            memo_started_ms: memo_ms,
            memo_more: false,
            final_started: false,
            last_start: None,
        }
    }

    pub fn final_pending(&self, poll: &Poll) -> bool {
        poll.has_text && poll.memo_changed && (!self.final_started || self.memo_more)
    }

    pub fn next(&self, poll: &Poll) -> Option<Job> {
        if !poll.has_text {
            return None;
        }
        if poll.ending {
            let gap_passed = self.last_start.is_none_or(|last| poll.now >= last + self.final_gap);
            return (self.final_pending(poll) && (self.final_started || gap_passed))
                .then_some(Job::Memo { last: true });
        }
        if !self.hint_attempted {
            return Some(Job::Hint);
        }
        if poll.paused {
            return None;
        }

        let hint_interval = self.hint_every.max(3 * self.hint.took);
        let hint_due = poll.hint_changed
            && self.hint.ready(poll.now)
            && self.hint.started.is_none_or(|last| poll.now >= last + hint_interval);

        // The first chunk gets priority. Give an overdue hint a slot between chunks.
        if self.memo_more {
            return Some(if hint_due { Job::Hint } else { Job::Memo { last: false } });
        }
        let memo_due = poll.memo_changed
            && self.memo.ready(poll.now)
            && (self.memo.failures > 0
                || self.memo_started_ms.is_none() && self.late_open
                || poll.meeting_ms >= self.memo_started_ms.unwrap_or(0) + MEMO_EVERY_MS);
        if memo_due { Some(Job::Memo { last: false }) } else { hint_due.then_some(Job::Hint) }
    }

    pub fn started(&mut self, job: Job, poll: &Poll) {
        self.last_start = Some(poll.now);
        match job {
            Job::Hint => {
                if !self.hint_attempted {
                    self.late_open = poll.covered_ms >= LATE_OPEN_MS;
                    self.hint_attempted = true;
                }
                self.hint.started = Some(poll.now);
            }
            Job::Memo { last } => {
                if !self.memo_more {
                    self.memo_started_ms = Some(poll.meeting_ms);
                }
                self.final_started |= last;
                self.memo.started = Some(poll.now);
            }
        }
    }

    pub fn finished(&mut self, job: Job, now: Duration, outcome: Outcome) {
        let success = matches!(outcome, Outcome::Succeeded { .. });
        match job {
            Job::Hint => self.hint.finished(now, success, self.hint_every),
            Job::Memo { .. } => {
                self.memo.finished(now, success, self.hint_every);
                self.memo_more = matches!(outcome, Outcome::Succeeded { more: true });
            }
        }
    }

    pub fn stale_hint(&self) -> bool {
        self.hint.failures >= 2
    }

    pub fn stale_memo(&self) -> bool {
        self.memo.failures > 0
    }
}

#[cfg(test)]
mod tests;
