use super::*;

fn poll(secs: u64, meeting_secs: u64) -> Poll {
    Poll {
        now: Duration::from_secs(secs),
        meeting_ms: meeting_secs * 1_000,
        covered_ms: meeting_secs * 1_000,
        has_text: true,
        hint_changed: true,
        memo_changed: true,
        paused: false,
        ending: false,
    }
}

fn scheduler() -> Scheduler {
    Scheduler::new(HINT_EVERY, FINAL_GAP, false, None)
}

fn hint(s: &mut Scheduler, at: &Poll, took: u64, success: bool) {
    s.started(Job::Hint, at);
    let outcome = if success { Outcome::Succeeded { more: false } } else { Outcome::Failed };
    s.finished(Job::Hint, at.now + Duration::from_secs(took), outcome);
}

const MEMO: Job = Job::Memo { last: false };
const FINAL: Job = Job::Memo { last: true };

#[test]
fn starts_immediately_and_hints_only_when_new_text_and_time_allow() {
    let mut s = scheduler();
    assert_eq!(s.next(&Poll { has_text: false, ..poll(0, 0) }), None);
    assert_eq!(s.next(&poll(0, 0)), Some(Job::Hint));
    hint(&mut s, &poll(0, 0), 1, true);
    for (poll, expected) in [
        (poll(29, 29), None),
        (poll(30, 30), Some(Job::Hint)),
        (Poll { hint_changed: false, memo_changed: false, ..poll(600, 600) }, None),
        (poll(120, 120), Some(Job::Hint)),
        (Poll { paused: true, ..poll(300, 300) }, None),
        (poll(299, 299), Some(Job::Hint)),
        (poll(300, 300), Some(MEMO)),
    ] {
        assert_eq!(s.next(&poll), expected);
    }
}

#[test]
fn a_late_open_writes_a_hint_then_a_memo_even_if_the_hint_failed() {
    for success in [true, false] {
        let mut s = scheduler();
        assert_eq!(s.next(&poll(0, 130)), Some(Job::Hint));
        hint(&mut s, &poll(0, 130), 1, success);
        assert_eq!(s.next(&poll(1, 131)), Some(MEMO));
    }
}

#[test]
fn the_memo_takes_the_first_slot_and_yields_between_chunks() {
    let mut s = scheduler();
    hint(&mut s, &poll(0, 0), 1, true);
    assert_eq!(s.next(&poll(300, 300)), Some(MEMO));
    s.started(MEMO, &poll(300, 300));
    s.finished(MEMO, Duration::from_secs(301), Outcome::Succeeded { more: true });
    assert_eq!(s.next(&poll(301, 301)), Some(Job::Hint));
    hint(&mut s, &poll(301, 301), 1, true);
    assert_eq!(s.next(&poll(302, 302)), Some(MEMO));
    s.started(MEMO, &poll(302, 302));
    s.finished(MEMO, Duration::from_secs(310), Outcome::Succeeded { more: false });
    assert_eq!(s.next(&poll(599, 599)), Some(Job::Hint));
    assert_eq!(s.next(&poll(600, 600)), Some(MEMO));
}

#[test]
fn pausing_allows_just_one_attempt_and_ending_allows_just_one_final_batch() {
    let mut s = scheduler();
    let paused = Poll { paused: true, ..poll(0, 0) };
    assert_eq!(s.next(&paused), Some(Job::Hint));
    hint(&mut s, &paused, 1, false);
    assert_eq!(s.next(&Poll { paused: true, ..poll(300, 300) }), None);

    let end = |secs| Poll { ending: true, ..poll(secs, 4) };
    assert_eq!(s.next(&end(4)), None);
    assert!(s.final_pending(&end(4)));
    assert_eq!(s.next(&end(5)), Some(FINAL));
    s.started(FINAL, &end(5));
    s.finished(FINAL, Duration::from_secs(6), Outcome::Failed);
    assert_eq!(s.next(&end(1000)), None);

    assert_eq!(scheduler().next(&end(0)), Some(FINAL));
    assert_eq!(scheduler().next(&Poll { has_text: false, ..end(0) }), None);
    assert_eq!(scheduler().next(&Poll { memo_changed: false, ..end(1) }), None);
}

#[test]
fn a_twenty_second_hint_pushes_the_next_start_to_sixty_seconds() {
    let mut s = scheduler();
    hint(&mut s, &poll(0, 0), 20, true);
    assert_eq!(s.next(&poll(59, 59)), None);
    assert_eq!(s.next(&poll(60, 60)), Some(Job::Hint));
}

#[test]
fn each_kind_backs_off_independently_to_five_minutes_and_success_resets_it() {
    for job in [Job::Hint, MEMO] {
        let mut s = scheduler();
        hint(&mut s, &poll(0, 0), 1, true);
        let mut at = 30;
        let p = |secs| Poll { hint_changed: job == Job::Hint, memo_changed: job == MEMO, ..poll(secs, 600) };
        for (failures, delay) in [30, 60, 120, 240, 300, 300].into_iter().enumerate() {
            assert_eq!(s.next(&p(at)), Some(job));
            s.started(job, &p(at));
            s.finished(job, Duration::from_secs(at + 1), Outcome::Failed);
            assert_eq!(s.stale_hint(), job == Job::Hint && failures > 0);
            assert_eq!(s.stale_memo(), job == MEMO);
            let other = Poll { hint_changed: job == MEMO, memo_changed: job == Job::Hint, ..poll(at + 1, 600) };
            assert_eq!(s.next(&other), Some(if job == MEMO { Job::Hint } else { MEMO }));
            assert_eq!(s.next(&p(at + delay)), None);
            at += 1 + delay;
        }
        s.started(job, &p(at));
        s.finished(job, Duration::from_secs(at + 1), Outcome::Succeeded { more: false });
        assert!(!s.stale_hint() && !s.stale_memo());
        at += 300;
        let later = |secs| Poll { meeting_ms: 900_000, ..p(secs) };
        assert_eq!(s.next(&later(at)), Some(job));
        s.started(job, &later(at));
        s.finished(job, Duration::from_secs(at + 1), Outcome::Failed);
        assert_eq!(s.next(&later(at + 30)), None);
        assert_eq!(s.next(&later(at + 31)), Some(job));
    }
}
