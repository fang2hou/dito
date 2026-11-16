//! The motion of switching chats (`design:tools/chat`, 23.05.6): the turns on screen fade out from top to bottom, then
//! the chosen chat's turns rise in one at a time. The poses are a pure function of the time since the switch began.

use std::ops::Range;
use std::time::Instant;

use dito_core::store::{Author, ChatMessageRow};

const OUT_MS: f32 = 120.;
const OUT_STAGGER_MS: f32 = 30.;
const IN_MS: f32 = 240.;
const IN_STAGGER_MS: f32 = 80.;
const IN_RISE: f32 = 8.;
const REDUCED_HALF_MS: f32 = 50.;
/// Only the turns at the bottom of the view animate, and never more than this many.
pub(super) const MAX_TURNS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Pose {
    pub opacity: f32,
    /// Points the turn still has to rise.
    pub rise: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    /// The chat being left is shown.
    Out,
    /// The chosen chat is shown.
    In,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Poses {
    pub phase: Phase,
    /// One per animated turn, top to bottom. Turns above them take the first one's opacity.
    pub turns: Vec<Pose>,
}

pub(super) struct Switch {
    start: Instant,
    /// The opacity each outgoing turn started fading from, so a switch cut short fades out from where it was.
    from: Vec<f32>,
    incoming: usize,
    reduced: bool,
}

fn clamp01(x: f32) -> f32 {
    x.clamp(0., 1.)
}

impl Switch {
    pub fn new(start: Instant, from: Vec<f32>, incoming: usize, reduced: bool) -> Self {
        Self { start, from, incoming: incoming.min(MAX_TURNS), reduced }
    }

    fn out_end_ms(&self) -> f32 {
        match self.from.len() {
            0 => 0.,
            n => (n - 1) as f32 * OUT_STAGGER_MS + OUT_MS,
        }
    }

    fn in_end_ms(&self) -> f32 {
        match self.incoming {
            0 => 0.,
            n => (n - 1) as f32 * IN_STAGGER_MS + IN_MS,
        }
    }

    pub fn total_ms(&self) -> f32 {
        if self.reduced { 2. * REDUCED_HALF_MS } else { self.out_end_ms() + self.in_end_ms() }
    }

    pub fn elapsed_ms(&self, now: Instant) -> f32 {
        now.saturating_duration_since(self.start).as_secs_f32() * 1000.
    }

    /// `None` once the switch has settled.
    pub fn poses(&self, elapsed_ms: f32) -> Option<Poses> {
        if elapsed_ms >= self.total_ms() {
            return None;
        }
        if self.reduced {
            return Some(self.reduced_poses(elapsed_ms));
        }
        let out_end = self.out_end_ms();
        if elapsed_ms < out_end {
            let turns = self
                .from
                .iter()
                .enumerate()
                .map(|(i, from)| {
                    let x = clamp01((elapsed_ms - i as f32 * OUT_STAGGER_MS) / OUT_MS);
                    Pose { opacity: from * (1. - x * x), rise: 0. }
                })
                .collect();
            return Some(Poses { phase: Phase::Out, turns });
        }
        let into = elapsed_ms - out_end;
        let turns = (0..self.incoming)
            .map(|j| {
                let x = clamp01((into - j as f32 * IN_STAGGER_MS) / IN_MS);
                let eased = 1. - (1. - x) * (1. - x) * (1. - x);
                Pose { opacity: eased, rise: IN_RISE * (1. - eased) }
            })
            .collect();
        Some(Poses { phase: Phase::In, turns })
    }

    /// Reduce Motion: the old chat fades out and the new one fades in, 100 ms in all, without moving.
    fn reduced_poses(&self, elapsed_ms: f32) -> Poses {
        if elapsed_ms < REDUCED_HALF_MS {
            let x = elapsed_ms / REDUCED_HALF_MS;
            let turns = self.from.iter().map(|from| Pose { opacity: from * (1. - x), rise: 0. }).collect();
            Poses { phase: Phase::Out, turns }
        } else {
            let x = (elapsed_ms - REDUCED_HALF_MS) / REDUCED_HALF_MS;
            Poses { phase: Phase::In, turns: vec![Pose { opacity: x, rise: 0. }; self.incoming] }
        }
    }
}

/// Each user message with the reply that follows it, as index ranges; a reply without a question before it forms a
/// turn of its own.
pub(super) fn turns(messages: &[ChatMessageRow]) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    let mut i = 0;
    while i < messages.len() {
        let end = match (messages[i].author, messages.get(i + 1).map(|m| m.author)) {
            (Author::User, Some(Author::Assistant)) => i + 2,
            _ => i + 1,
        };
        out.push(i..end);
        i = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use dito_core::store::MessageState;

    use super::*;

    fn at(switch: &Switch, ms: f32) -> Option<Poses> {
        switch.poses(ms)
    }

    fn opacities(poses: &Poses) -> Vec<f32> {
        poses.turns.iter().map(|t| t.opacity).collect()
    }

    #[test]
    fn two_turns_fade_out_top_first_then_rise_in_80_ms_apart() {
        let s = Switch::new(Instant::now(), vec![1., 1.], 2, false);
        let start = at(&s, 0.).unwrap();
        assert_eq!((start.phase, opacities(&start)), (Phase::Out, vec![1., 1.]));

        let mid = at(&s, 60.).unwrap();
        assert!(mid.turns[0].opacity < mid.turns[1].opacity, "the top turn leads");
        assert_eq!(mid.turns[1].opacity, 1. - 0.25 * 0.25, "30 ms in a 120 ms fade");

        let incoming = at(&s, 150.).unwrap();
        assert_eq!(incoming.phase, Phase::In);
        assert_eq!(incoming.turns[0], Pose { opacity: 0., rise: IN_RISE });

        let second = at(&s, 150. + 80.).unwrap();
        assert_eq!(second.turns[1].opacity, 0., "the second turn starts 80 ms after the first");
        assert!(second.turns[0].opacity > 0.);

        assert_eq!(s.total_ms(), 470.);
        assert!(at(&s, 470.).is_none());
        let last = at(&s, 469.).unwrap();
        assert!(last.turns.iter().all(|t| t.opacity > 0.99 && t.rise < 0.1));
    }

    #[test]
    fn a_switch_cut_short_fades_out_from_the_opacity_it_had() {
        let s = Switch::new(Instant::now(), vec![0.4, 1.], 1, false);
        assert_eq!(opacities(&at(&s, 0.).unwrap()), vec![0.4, 1.]);
        assert!(at(&s, 100.).unwrap().turns[0].opacity < 0.4);
    }

    #[test]
    fn an_empty_side_skips_its_phase() {
        let from_empty = Switch::new(Instant::now(), vec![], 2, false);
        assert_eq!(at(&from_empty, 0.).unwrap().phase, Phase::In);
        let to_empty = Switch::new(Instant::now(), vec![1.], 0, false);
        assert_eq!(to_empty.total_ms(), OUT_MS);
        assert!(at(&to_empty, OUT_MS).is_none());
        assert!(at(&Switch::new(Instant::now(), vec![], 0, false), 0.).is_none());
    }

    #[test]
    fn only_the_last_turns_animate_and_reduce_motion_never_moves() {
        let s = Switch::new(Instant::now(), vec![1.; 3], 9, false);
        assert_eq!(s.incoming, MAX_TURNS);
        let reduced = Switch::new(Instant::now(), vec![1.; 2], 2, true);
        assert_eq!(reduced.total_ms(), 100.);
        assert!(at(&reduced, 25.).unwrap().turns.iter().all(|t| t.rise == 0. && t.opacity == 0.5));
        let shown = at(&reduced, 75.).unwrap();
        assert_eq!((shown.phase, opacities(&shown)), (Phase::In, vec![0.5, 0.5]));
        assert!(at(&reduced, 100.).is_none());
        let _ = Duration::ZERO;
    }

    fn row(author: Author) -> ChatMessageRow {
        ChatMessageRow {
            id: 0,
            author,
            content: String::new(),
            state: MessageState::Done,
            error: None,
            attachments: vec![],
            model: None,
            ts: 0,
        }
    }

    #[test]
    fn a_turn_is_a_question_with_the_reply_after_it() {
        use Author::{Assistant as A, User as U};
        let messages: Vec<_> = [U, A, U, U, A].into_iter().map(row).collect();
        assert_eq!(turns(&messages), vec![0..2, 2..3, 3..5]);
        assert!(turns(&[]).is_empty());
    }
}
