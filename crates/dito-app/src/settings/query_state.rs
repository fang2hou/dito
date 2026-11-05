//! Pure request-state for the settings pages' data loads: active gating, one query in flight
//! per page, refresh coalescing that cannot starve slow queries, input-change invalidation,
//! and invalidation on page leave. No GPUI or AppState dependency, so the lifecycle policy
//! the pages run is testable directly.

/// What a finished query's `finish` means for its result.
#[derive(Debug, PartialEq, Eq)]
pub enum Finish {
    /// The result matches the newest data version: apply it.
    Apply,
    /// Same-input refreshes were collapsed while this one ran: apply this result, then launch
    /// exactly one more query with the returned ticket.
    ApplyAndReload(u64),
    /// The inputs changed under this query: drop its result and launch the replacement with
    /// this ticket.
    Superseded(u64),
    /// Dropped without applying: the page is inactive, the state was invalidated, or the
    /// ticket is unknown.
    Ignored,
}

/// What to do when the running query finishes.
#[derive(Debug, Default, PartialEq, Eq)]
enum Pending {
    #[default]
    None,
    /// Same-input refresh queued: the finishing result still applies, one reload follows.
    Refresh,
    /// Input change queued: the finishing result is stale and must be dropped.
    Restart,
}

/// How a finished result affected the page's snapshot.
#[derive(Debug)]
pub enum Snapshot {
    /// Data changed and was stored; the page re-renders.
    Replaced,
    /// Same data as before; nothing to do.
    Unchanged,
    /// The query failed: the last successful data stays and the error is kept for logging.
    Failed(anyhow::Error),
}

/// Ticketed single-flight request state with an active gate. `request` (same-input refresh)
/// never invalidates the running result, so a query that is consistently slower than the
/// refresh interval still publishes; `restart` (input change) does. Deactivating invalidates
/// everything in flight, so results that land after a page leave never apply.
#[derive(Debug, Default)]
pub struct QueryState {
    active: bool,
    /// Newest requested data version; results from older versions are stale.
    generation: u64,
    /// Ticket of the one running query, if any.
    inflight: Option<u64>,
    /// Coalesced follow-up for when the running query finishes.
    pending: Pending,
}

impl QueryState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Page visibility lifecycle. Activating is idempotent; deactivating invalidates every
    /// outstanding ticket so late results are ignored until the page is active again.
    pub fn set_active(&mut self, active: bool) {
        if self.active == active {
            return;
        }
        self.active = active;
        if !active {
            self.invalidate();
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Same-input refresh (poll tick, action completion, page entry). Returns a ticket to
    /// launch now, or `None` when a query is already running — this refresh collapsed into
    /// it and at most one reload will follow it. A queued input change always outranks a
    /// refresh, so a refresh can never un-stale a result that must be dropped.
    pub fn request(&mut self) -> Option<u64> {
        self.enqueue(false)
    }

    /// Input change (search text, period, style, range). Like `request`, but the result of
    /// any query still running under the old inputs is marked stale and dropped on arrival.
    pub fn restart(&mut self) -> Option<u64> {
        self.enqueue(true)
    }

    fn enqueue(&mut self, restart: bool) -> Option<u64> {
        if !self.active {
            return None;
        }
        self.generation += 1;
        if self.inflight.is_some() {
            self.pending =
                if restart || self.pending == Pending::Restart { Pending::Restart } else { Pending::Refresh };
            None
        } else {
            self.inflight = Some(self.generation);
            Some(self.generation)
        }
    }

    /// Report the query for `ticket` finished. See [`Finish`] for how to act on each case.
    /// After deactivation every finish is `Ignored`.
    pub fn finish(&mut self, ticket: u64) -> Finish {
        if self.inflight != Some(ticket) {
            return Finish::Ignored;
        }
        self.inflight = None;
        let pending = std::mem::take(&mut self.pending);
        if pending == Pending::None {
            return Finish::Apply;
        }
        self.generation += 1;
        self.inflight = Some(self.generation);
        match pending {
            Pending::Refresh => Finish::ApplyAndReload(self.generation),
            Pending::Restart => Finish::Superseded(self.generation),
            Pending::None => unreachable!("handled above"),
        }
    }

    /// Drop all in-flight bookkeeping and obsolete every outstanding ticket. The next
    /// `request` starts fresh.
    pub fn invalidate(&mut self) {
        self.inflight = None;
        self.pending = Pending::None;
        self.generation += 1;
    }
}

/// The two conditions for hiding the history skeleton: the minimum navigation-settle delay
/// and the first query outcome — success or error, so a failed first load cannot spin the
/// loader forever. Entry generations make a timer from an earlier visit unable to clear a
/// newer entry's skeleton.
#[derive(Debug, Default)]
pub struct SkeletonGate {
    epoch: u64,
    settle: bool,
    outcome: bool,
}

impl SkeletonGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// A page entry: resets both conditions and invalidates the timers of earlier entries.
    /// Returns the entry's generation, to be handed back by its settle timer.
    pub fn enter(&mut self) -> u64 {
        self.settle = false;
        self.outcome = false;
        self.epoch += 1;
        self.epoch
    }

    /// The settle timer of `epoch` fired. `true` only when that timer belongs to the newest
    /// entry and the first outcome has already landed.
    pub fn settle_elapsed(&mut self, epoch: u64) -> bool {
        if epoch != self.epoch {
            return false;
        }
        self.settle = true;
        self.open()
    }

    /// The first query outcome landed (data or error — the flip is the same). `true` only
    /// when the settle delay has already elapsed.
    pub fn outcome_received(&mut self) -> bool {
        self.outcome = true;
        self.open()
    }

    fn open(&self) -> bool {
        self.settle && self.outcome
    }
}

/// Shared apply policy: a successful query replaces `current` only when the data actually
/// changed; a failure keeps the last successful data and hands back the error instead of
/// degrading to an empty page.
pub fn apply_snapshot<T: PartialEq>(current: &mut T, result: anyhow::Result<T>) -> Snapshot {
    match result {
        Ok(new) if &new == current => Snapshot::Unchanged,
        Ok(new) => {
            *current = new;
            Snapshot::Replaced
        }
        Err(e) => Snapshot::Failed(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active_state() -> QueryState {
        let mut q = QueryState::new();
        q.set_active(true);
        q
    }

    // Inactive pages launch nothing, and a leave invalidates whatever is in flight.
    #[test]
    fn inactive_page_requests_nothing_and_leave_invalidates_inflight() {
        let mut q = QueryState::new();
        assert_eq!(q.request(), None, "never active: no queries");
        assert_eq!(q.restart(), None, "never active: no queries");

        q.set_active(true);
        let ticket = q.request().expect("active page launches");
        q.set_active(false);
        assert_eq!(q.finish(ticket), Finish::Ignored, "result after leave is ignored");
        assert_eq!(q.request(), None, "inactive page starts no new query");
    }

    // Reentering a page starts fresh: the stale ticket never applies, a new one does.
    #[test]
    fn reentry_ignores_stale_and_applies_fresh() {
        let mut q = active_state();
        let stale = q.request().unwrap();
        q.set_active(false);
        q.set_active(true);
        assert_eq!(q.finish(stale), Finish::Ignored);
        let fresh = q.request().expect("first request after reentry launches");
        assert_ne!(fresh, stale);
        assert_eq!(q.finish(fresh), Finish::Apply);
    }

    // Slow old query vs fast new one: the old result must be dropped, the new one applied.
    #[test]
    fn input_change_supersedes_slow_older_result() {
        let mut q = active_state();
        let old = q.request().unwrap();
        assert_eq!(q.restart(), None, "input change collapses while the first runs");
        let Finish::Superseded(new) = q.finish(old) else { panic!("old result must be dropped") };
        assert_ne!(new, old);
        assert_eq!(q.finish(old), Finish::Ignored, "a repeated finish of the old ticket is ignored");
        assert_eq!(q.finish(new), Finish::Apply);
    }

    // A refresh queued first must not un-stale a result an input change invalidated.
    #[test]
    fn input_change_outranks_queued_refresh() {
        let mut q = active_state();
        let first = q.request().unwrap();
        assert_eq!(q.request(), None, "refresh collapses");
        assert_eq!(q.restart(), None, "input change collapses but marks the result stale");
        let Finish::Superseded(_) = q.finish(first) else {
            panic!("result under old inputs must be dropped despite the queued refresh")
        };
    }

    // A query slower than the refresh interval still publishes on every completion; the
    // refresh stream can never starve the page of data.
    #[test]
    fn sustained_slow_queries_still_publish() {
        let mut q = active_state();
        let mut ticket = q.request().unwrap();
        for round in 0..5 {
            // Ticks keep landing while the slow query runs.
            assert_eq!(q.request(), None, "round {round}: tick collapses");
            assert_eq!(q.request(), None, "round {round}: further ticks change nothing");
            let Finish::ApplyAndReload(next) = q.finish(ticket) else {
                panic!("round {round}: result must publish, not be superseded")
            };
            ticket = next;
        }
        assert_eq!(q.finish(ticket), Finish::Apply, "no pending reload without a tick");
    }

    // Bursts of refresh ticks collapse into at most one reload per query completion.
    #[test]
    fn refresh_bursts_collapse_into_one_reload_each() {
        let mut q = active_state();
        let first = q.request().unwrap();
        assert_eq!(q.request(), None);
        assert_eq!(q.request(), None);
        assert_eq!(q.request(), None);
        let Finish::ApplyAndReload(second) = q.finish(first) else { panic!("expected one reload") };
        // Four requests produced exactly two queries: the original plus one collapsed reload.
        assert_eq!(q.finish(second), Finish::Apply);
    }

    // The skeleton hides only after both the settle delay and the first outcome — in either
    // order. This is the exact logic HistoryPage::enter/apply run.
    #[test]
    fn skeleton_opens_after_settle_then_first_outcome() {
        let mut g = SkeletonGate::new();
        let entry = g.enter();
        assert!(!g.settle_elapsed(entry), "settle alone is not enough");
        assert!(g.outcome_received(), "the first outcome completes the pair");
    }

    #[test]
    fn skeleton_opens_after_outcome_then_settle() {
        let mut g = SkeletonGate::new();
        let entry = g.enter();
        assert!(!g.outcome_received(), "outcome alone is not enough");
        assert!(g.settle_elapsed(entry), "the settle timer completes the pair");
    }

    // A settle timer from an earlier visit must not clear a newer entry's skeleton.
    #[test]
    fn stale_entry_timer_cannot_clear_newer_skeleton() {
        let mut g = SkeletonGate::new();
        let old = g.enter();
        let new = g.enter();
        assert!(!g.settle_elapsed(old), "timer of a superseded entry is ignored");
        assert!(!g.outcome_received(), "new entry has not settled yet");
        assert!(g.settle_elapsed(new), "the newer entry's own timer opens the gate");
    }

    // A re-entered page starts from a clean slate: neither condition carries over.
    #[test]
    fn reentry_resets_both_conditions() {
        let mut g = SkeletonGate::new();
        let first = g.enter();
        assert!(!g.settle_elapsed(first));
        assert!(g.outcome_received(), "first stay fully opened");
        let second = g.enter();
        assert!(!g.settle_elapsed(second), "settle alone still does not open a fresh entry");
        assert!(g.outcome_received(), "pair completes again after both fresh conditions");
    }

    // A failed first query uses the same flip as success: the loader must not spin forever.
    #[test]
    fn failed_first_outcome_also_ends_skeleton() {
        let mut g = SkeletonGate::new();
        let entry = g.enter();
        assert!(!g.settle_elapsed(entry));
        assert!(g.outcome_received(), "error outcomes open the gate too");
    }

    // A failed query keeps the last successful data; it must not read as an empty success.
    #[test]
    fn failed_query_keeps_last_data() {
        let mut page = vec!["old".to_string()];
        match apply_snapshot(&mut page, Err(anyhow::anyhow!("db locked"))) {
            Snapshot::Failed(_) => {}
            other => panic!("expected failure, got {other:?}"),
        }
        assert_eq!(page, ["old".to_string()]);
    }

    #[test]
    fn successful_empty_result_replaces_data() {
        let mut page = vec!["old".to_string()];
        assert!(matches!(apply_snapshot(&mut page, Ok(Vec::new())), Snapshot::Replaced));
        assert!(page.is_empty());
    }

    #[test]
    fn equal_result_is_unchanged() {
        let mut page = vec![7u32];
        assert!(matches!(apply_snapshot(&mut page, Ok(vec![7u32])), Snapshot::Unchanged));
    }

    // Arc snapshots compare by content and swap by handle — the shape the history page uses.
    #[test]
    fn arc_snapshot_swaps_handle_only_on_content_change() {
        let mut page = std::sync::Arc::new(vec!["a".to_string()]);
        let same = std::sync::Arc::new(vec!["a".to_string()]);
        assert!(matches!(apply_snapshot(&mut page, Ok(same)), Snapshot::Unchanged));
        let next = std::sync::Arc::new(vec!["b".to_string()]);
        assert!(matches!(apply_snapshot(&mut page, Ok(next)), Snapshot::Replaced));
        assert_eq!(*page, ["b".to_string()]);
    }
}
