//! What is still being heard, per speaker: the words of utterances not yet finally recognized. The session turns it
//! and the open paragraph into one [`LiveText`] per speaker, published in a single status update together with any
//! paragraph that closed, so the view never sees words in two places or in none.

use super::Speaker;
use super::paragraphs::{Paragraph, join};

/// The paragraph still being heard for one speaker, as the page shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveText {
    pub speaker: Speaker,
    /// Milliseconds since the session started.
    pub start_ms: u64,
    /// Words recognized from finished utterances: final, waiting only for the paragraph to close.
    pub settled: String,
    /// Words of utterances still going on or still being recognized; they may change. Joined so that
    /// `settled + unsettled` reads right (a space between words of a spaced language).
    pub unsettled: String,
}

struct Pending {
    speaker: Speaker,
    seq: u64,
    start_ms: u64,
    text: String,
    /// The utterance ended and its final recognition is on its way.
    ended: bool,
}

#[derive(Default)]
pub(super) struct Hearing {
    /// In the order the utterances started, per speaker.
    pending: Vec<Pending>,
    /// Per speaker, the last utterance whose final text arrived; anything older is stale.
    finished: Vec<(Speaker, u64)>,
}

impl Hearing {
    fn finished(&self, speaker: Speaker) -> u64 {
        self.finished.iter().find(|(s, _)| *s == speaker).map_or(0, |(_, seq)| *seq)
    }

    /// Partial text of utterance `seq`. Returns whether anything shown changed.
    pub fn partial(&mut self, speaker: Speaker, seq: u64, start_ms: u64, text: String) -> bool {
        if seq <= self.finished(speaker) {
            return false;
        }
        match self.pending.iter_mut().find(|p| p.speaker == speaker && p.seq == seq) {
            Some(p) if p.text == text => false,
            Some(p) => {
                p.text = text;
                true
            }
            None => {
                let at = self.pending.iter().position(|p| p.speaker == speaker && p.seq > seq);
                let p = Pending { speaker, seq, start_ms, text, ended: false };
                match at {
                    Some(i) => self.pending.insert(i, p),
                    None => self.pending.push(p),
                }
                true
            }
        }
    }

    /// The utterance ended; its words stay shown until its final text replaces them. One too short for any partial
    /// text is still pending, so the paragraph waits for it.
    pub fn ended(&mut self, speaker: Speaker, seq: u64, start_ms: u64) {
        if seq <= self.finished(speaker) {
            return;
        }
        match self.pending.iter_mut().find(|p| p.speaker == speaker && p.seq == seq) {
            Some(p) => p.ended = true,
            None => self.pending.push(Pending { speaker, seq, start_ms, text: String::new(), ended: true }),
        }
    }

    /// Speech that never became an utterance. Returns whether anything shown changed.
    pub fn dropped(&mut self, speaker: Speaker, seq: u64) -> bool {
        let before = self.pending.len();
        self.pending.retain(|p| !(p.speaker == speaker && p.seq == seq && !p.ended));
        self.pending.len() != before
    }

    /// When the partial text of `seq` said the utterance started. It counts from just before speech was noticed, so
    /// it can be earlier than the detector's own start; the earlier one is kept so a shown time never moves later.
    pub fn start_of(&self, speaker: Speaker, seq: u64) -> Option<u64> {
        self.pending.iter().find(|p| p.speaker == speaker && p.seq == seq).map(|p| p.start_ms)
    }

    /// The final text of `seq` arrived: it and anything older of that speaker is no longer pending.
    pub fn finished_utterance(&mut self, speaker: Speaker, seq: u64) {
        self.pending.retain(|p| !(p.speaker == speaker && p.seq <= seq));
        match self.finished.iter_mut().find(|(s, _)| *s == speaker) {
            Some((_, last)) => *last = (*last).max(seq),
            None => self.finished.push((speaker, seq)),
        }
    }

    /// Something of this speaker is still on its way.
    pub fn busy(&self, speaker: Speaker) -> bool {
        self.pending.iter().any(|p| p.speaker == speaker)
    }

    /// One entry per speaker with something to show: the open paragraph's speaker first.
    pub fn view(&self, open: Option<&Paragraph>) -> Vec<LiveText> {
        let mut out: Vec<LiveText> = Vec::new();
        if let Some(p) = open {
            out.push(LiveText {
                speaker: p.speaker,
                start_ms: p.start_ms,
                settled: p.text.clone(),
                unsettled: String::new(),
            });
        }
        for p in self.pending.iter().filter(|p| !p.text.is_empty()) {
            let i = match out.iter().position(|e| e.speaker == p.speaker) {
                Some(i) => i,
                None => {
                    out.push(LiveText {
                        speaker: p.speaker,
                        start_ms: p.start_ms,
                        settled: String::new(),
                        unsettled: String::new(),
                    });
                    out.len() - 1
                }
            };
            let e = &mut out[i];
            let mut whole = format!("{}{}", e.settled, e.unsettled);
            join(&mut whole, &p.text);
            e.unsettled = whole.split_off(e.settled.len());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heard_words_stay_until_their_final_text_replaces_them() {
        let mut h = Hearing::default();
        let open =
            Paragraph { speaker: Speaker::Others, start_ms: 0, end_ms: 3_000, text: "Shipping is Thursday.".into() };
        assert!(h.partial(Speaker::Others, 1, 3_000, "Tests are".into()));
        assert!(!h.partial(Speaker::Others, 1, 3_000, "Tests are".into()), "the same text changes nothing");
        h.ended(Speaker::Others, 1, 3_000);
        assert!(h.partial(Speaker::Others, 2, 4_000, "and then".into()));
        assert!(!h.dropped(Speaker::Others, 1), "an ended utterance waits for its final text");
        let [e] = &h.view(Some(&open))[..] else { panic!("one speaker") };
        assert_eq!(format!("{}{}", e.settled, e.unsettled), "Shipping is Thursday. Tests are and then");

        h.finished_utterance(Speaker::Others, 1);
        assert!(!h.partial(Speaker::Others, 1, 3_000, "late".into()), "a partial older than the final text is ignored");
        let [e] = &h.view(None)[..] else { panic!("one speaker") };
        assert_eq!((e.start_ms, e.unsettled.as_str()), (4_000, "and then"));
        assert!(h.busy(Speaker::Others) && !h.busy(Speaker::Me));
        assert!(h.dropped(Speaker::Others, 2));
        assert!(h.view(None).is_empty());
    }
}
