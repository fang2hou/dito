//! Pure decision logic for text insertion: chunking for synthetic key events, verification of
//! what actually landed, and the type → paste → clipboard fallback flow. The macOS FFI lives in
//! `dito_platform::insert`; this module holds everything that can be tested without a GUI.

use crate::config::InsertMethod;

/// Keep each synthesized key event small: some apps drop long unicode payloads.
pub const TYPE_CHUNK_UNITS: usize = 16;

/// Gap between chunks. 2 ms beat a 60 Hz main thread (one frame is 16.7 ms), letting event
/// coalescing in Chromium/Electron and Java apps drop whole chunks; 10 ms stays well under a
/// frame while giving slow consumers time to drain the event queue.
pub const TYPE_CHUNK_GAP_MS: u64 = 10;

/// Iterator over `units` in chunks of at most `max` UTF-16 units, never splitting a surrogate
/// pair (unless `max == 1`, where splitting is unavoidable — the pair still round-trips through
/// apps that buffer, and the alternative would be dropping units).
#[derive(Debug, Clone)]
pub struct Utf16Chunks<'a> {
    units: &'a [u16],
    start: usize,
    max: usize,
}

impl<'a> Utf16Chunks<'a> {
    pub fn new(units: &'a [u16], max: usize) -> Self {
        Self { units, start: 0, max: max.max(1) }
    }
}

impl<'a> Iterator for Utf16Chunks<'a> {
    type Item = &'a [u16];

    fn next(&mut self) -> Option<Self::Item> {
        if self.start >= self.units.len() {
            return None;
        }
        let mut end = (self.start + self.max).min(self.units.len());
        // Don't leave a high surrogate at the chunk end when its low half follows: back the
        // boundary off so the pair travels in one event (only possible when max >= 2).
        if end < self.units.len() && end - self.start > 1 && (0xDC00..0xE000).contains(&self.units[end]) {
            end -= 1;
        }
        let start = self.start;
        self.start = end;
        Some(&self.units[start..end])
    }
}

/// What an insertion attempt's verification showed. `Refuted.inserted` is the number of UTF-16
/// units observed landing during this attempt (0 = nothing landed, partial = some chunks dropped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The target field's selection moved by at least the expected amount.
    Confirmed,
    /// The AX cursor could not be read before or after: retrying would risk duplicating text
    /// that may have landed, so the attempt is accepted.
    Unknown,
    Refuted {
        inserted: usize,
    },
}

/// Verdict from the selection locations around one attempt. `expected` is the UTF-16 length of
/// the attempted text.
pub fn outcome(expected: usize, before: Option<usize>, after: Option<usize>) -> Outcome {
    match (before, after) {
        (Some(before), Some(after)) => {
            let inserted = after.saturating_sub(before);
            if inserted >= expected { Outcome::Confirmed } else { Outcome::Refuted { inserted } }
        }
        _ => Outcome::Unknown,
    }
}

/// One planned insertion attempt. `Type { from }` types the UTF-16 suffix starting at `from`
/// (`from > 0` means continuing after a partial insert).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Type { from: usize },
    Paste,
}

/// Which mechanism an attempt used, independent of the continuation offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    Type,
    Paste,
}

impl Step {
    pub fn kind(self) -> StepKind {
        match self {
            Step::Type { .. } => StepKind::Type,
            Step::Paste => StepKind::Paste,
        }
    }
}

/// The next move after an attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Verified, or unverifiable without duplication risk.
    Done,
    Attempt(Step),
    /// Live insertion is not working; leave the text on the clipboard.
    CopyToClipboard,
}

/// Retry policy across one delivery: the configured method first, at most one attempt with the
/// other method, and at most one continuation typing the missing suffix after a partial insert.
/// Once anything has landed, the whole text is never retried (that would duplicate the prefix).
#[derive(Debug, Clone)]
pub struct Flow {
    used: [bool; 2], // [StepKind::Type, StepKind::Paste]
}

impl Flow {
    pub fn new(configured: InsertMethod) -> Self {
        let mut used = [false, false];
        used[Flow::first(configured).kind() as usize] = true;
        Self { used }
    }

    /// The first attempt for the configured method.
    pub fn first(configured: InsertMethod) -> Step {
        match configured {
            InsertMethod::KeyInput => Step::Type { from: 0 },
            InsertMethod::Paste => Step::Paste,
        }
    }

    fn other_unused(&self, kind: StepKind) -> Option<Step> {
        let (other, step) = match kind {
            StepKind::Type => (StepKind::Paste, Step::Paste),
            StepKind::Paste => (StepKind::Type, Step::Type { from: 0 }),
        };
        (!self.used[other as usize]).then_some(step)
    }

    /// Decide what follows `attempted` producing `result`. `Refuted.inserted` is always below
    /// the attempted length, so a continuation offset is always within the text.
    pub fn advance(&mut self, attempted: Step, result: Outcome) -> Next {
        if matches!(result, Outcome::Confirmed | Outcome::Unknown) {
            return Next::Done;
        }
        let Outcome::Refuted { inserted } = result else { unreachable!("handled above") };
        let next = match attempted {
            // Fresh typing landed nothing: one attempt with the other mechanism.
            Step::Type { from: 0 } if inserted == 0 => self.other_unused(StepKind::Type).map(Next::Attempt),
            // Fresh typing landed part of it: continue with the missing suffix.
            Step::Type { from: 0 } => Some(Next::Attempt(Step::Type { from: inserted })),
            // A continuation never gets the whole text again, whatever it landed.
            Step::Type { .. } => None,
            // Paste landed nothing: one typing attempt. A partial paste cannot be continued
            // (no offset to resume from), and retyping would duplicate the pasted prefix.
            Step::Paste if inserted == 0 => self.other_unused(StepKind::Paste).map(Next::Attempt),
            Step::Paste => None,
        };
        match next {
            Some(Next::Attempt(step)) => {
                self.used[step.kind() as usize] = true;
                Next::Attempt(step)
            }
            _ => Next::CopyToClipboard,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunks(units: &[u16], max: usize) -> Vec<Vec<u16>> {
        Utf16Chunks::new(units, max).map(|c| c.to_vec()).collect()
    }

    #[test]
    fn chunks_respect_the_maximum_and_cover_everything() {
        let units: Vec<u16> = (0..40).map(|i| 0x41 + i as u16).collect();
        let cs = chunks(&units, 16);
        let sizes: Vec<usize> = cs.iter().map(|c| c.len()).collect();
        assert_eq!(sizes, [16, 16, 8]);
        assert!(cs.iter().flatten().eq(units.iter()));
    }

    #[test]
    fn empty_text_yields_no_chunks() {
        assert!(chunks(&[], 16).is_empty());
    }

    #[test]
    fn a_surrogate_pair_straddling_the_boundary_stays_together() {
        // 15 BMP units, then 🙂 (D83D DE00), then three more.
        let mut units: Vec<u16> = vec![0x61; 15];
        units.extend([0xD83D, 0xDE00]);
        units.extend([0x62; 3]);
        let cs = chunks(&units, 16);
        assert_eq!(cs[0].len(), 15, "the high surrogate must not end a chunk");
        assert_eq!(cs[1], [0xD83D, 0xDE00, 0x62, 0x62, 0x62]);
    }

    #[test]
    fn a_surrogate_pair_at_the_exact_boundary_is_not_split() {
        let mut units: Vec<u16> = (0..15u16).collect();
        units.extend([0xD83D, 0xDE00]);
        let cs = chunks(&units, 16);
        assert_eq!(cs.len(), 2);
        assert_eq!(cs[1], [0xD83D, 0xDE00]);
    }

    #[test]
    fn max_one_forces_pair_splitting_but_still_terminates() {
        assert_eq!(chunks(&[0xD83D, 0xDE00], 1), [vec![0xD83D], vec![0xDE00]]);
    }

    #[test]
    fn max_zero_is_treated_as_one() {
        assert_eq!(chunks(&[0x61, 0x62], 0), [vec![0x61], vec![0x62]]);
    }

    #[test]
    fn outcome_confirms_when_the_selection_moved_enough() {
        assert_eq!(outcome(7, Some(5), Some(12)), Outcome::Confirmed);
        assert_eq!(outcome(0, Some(5), Some(5)), Outcome::Confirmed);
    }

    #[test]
    fn outcome_refutes_with_the_observed_delta() {
        assert_eq!(outcome(10, Some(5), Some(8)), Outcome::Refuted { inserted: 3 });
        assert_eq!(outcome(4, Some(9), Some(5)), Outcome::Refuted { inserted: 0 });
    }

    #[test]
    fn unreadable_selections_are_unknown_never_refuted() {
        assert_eq!(outcome(4, None, Some(9)), Outcome::Unknown);
        assert_eq!(outcome(4, Some(9), None), Outcome::Unknown);
        assert_eq!(outcome(4, None, None), Outcome::Unknown);
    }

    #[test]
    fn typed_input_falls_through_to_paste_then_clipboard_when_nothing_lands() {
        let mut flow = Flow::new(InsertMethod::KeyInput);
        assert_eq!(Flow::first(InsertMethod::KeyInput), Step::Type { from: 0 });
        assert_eq!(flow.advance(Step::Type { from: 0 }, Outcome::Refuted { inserted: 0 }), Next::Attempt(Step::Paste));
        assert_eq!(flow.advance(Step::Paste, Outcome::Refuted { inserted: 0 }), Next::CopyToClipboard);
    }

    #[test]
    fn paste_first_falls_through_to_typing_then_clipboard() {
        let mut flow = Flow::new(InsertMethod::Paste);
        assert_eq!(Flow::first(InsertMethod::Paste), Step::Paste);
        assert_eq!(flow.advance(Step::Paste, Outcome::Refuted { inserted: 0 }), Next::Attempt(Step::Type { from: 0 }));
        assert_eq!(flow.advance(Step::Type { from: 0 }, Outcome::Refuted { inserted: 0 }), Next::CopyToClipboard);
    }

    #[test]
    fn a_partial_typing_continues_with_only_the_missing_suffix() {
        let mut flow = Flow::new(InsertMethod::KeyInput);
        assert_eq!(
            flow.advance(Step::Type { from: 0 }, Outcome::Refuted { inserted: 3 }),
            Next::Attempt(Step::Type { from: 3 })
        );
        // The continuation drops its own chunks: no more retries, the prefix must not be duplicated.
        assert_eq!(flow.advance(Step::Type { from: 3 }, Outcome::Refuted { inserted: 2 }), Next::CopyToClipboard);
    }

    #[test]
    fn a_stalled_continuation_never_restarts_the_whole_text() {
        let mut flow = Flow::new(InsertMethod::KeyInput);
        flow.advance(Step::Type { from: 0 }, Outcome::Refuted { inserted: 3 });
        assert_eq!(flow.advance(Step::Type { from: 3 }, Outcome::Refuted { inserted: 0 }), Next::CopyToClipboard);
    }

    #[test]
    fn a_partial_paste_goes_straight_to_clipboard() {
        let mut flow = Flow::new(InsertMethod::Paste);
        assert_eq!(flow.advance(Step::Paste, Outcome::Refuted { inserted: 4 }), Next::CopyToClipboard);
    }

    #[test]
    fn a_partial_fallback_typing_after_a_failed_paste_also_continues_once() {
        let mut flow = Flow::new(InsertMethod::Paste);
        flow.advance(Step::Paste, Outcome::Refuted { inserted: 0 });
        assert_eq!(
            flow.advance(Step::Type { from: 0 }, Outcome::Refuted { inserted: 6 }),
            Next::Attempt(Step::Type { from: 6 })
        );
        assert_eq!(flow.advance(Step::Type { from: 6 }, Outcome::Refuted { inserted: 0 }), Next::CopyToClipboard);
    }

    #[test]
    fn confirmed_or_unknown_attempts_end_the_flow() {
        let mut flow = Flow::new(InsertMethod::KeyInput);
        assert_eq!(flow.advance(Step::Type { from: 0 }, Outcome::Confirmed), Next::Done);
        assert_eq!(flow.advance(Step::Type { from: 0 }, Outcome::Unknown), Next::Done);
    }
}
