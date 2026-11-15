//! One audio source of a session: its detector thread cuts the stream into utterances and hands over the audio of
//! the utterance still going on, so it can be recognized before it ends.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self as std_mpsc, RecvTimeoutError};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use super::{Speaker, VoiceDetector};
use crate::audio::SAMPLE_RATE;

/// While someone speaks, the utterance so far is handed over each time it has grown this much. The recognizer only
/// keeps the newest one per speaker, so a slow model skips steps instead of falling behind.
pub(super) const PARTIAL_STEP: usize = SAMPLE_RATE as usize;
/// Audio kept from before the detector notices speech, since it notices a little late.
const PRE_ROLL: usize = SAMPLE_RATE as usize / 2;

/// What a source thread reports. `seq` numbers the utterances of one speaker in the order they started; the
/// partial audio and the end of one utterance share it.
pub(super) enum SourceEvent {
    /// The utterance so far, from just before it started.
    Partial { speaker: Speaker, seq: u64, start_ms: u64, samples: Vec<f32> },
    /// The utterance ended; its whole audio, timed from the session start.
    Ended { speaker: Speaker, seq: u64, start_ms: u64, end_ms: u64, samples: Vec<f32> },
    /// Speech stopped without becoming an utterance (too short): anything shown for it goes.
    Dropped { speaker: Speaker, seq: u64 },
}

/// Runs on the source's own thread until the audio stream ends, then flushes the speech in progress. `first_seq`
/// starts the utterance numbers, so a source reopened in the same session never reuses one.
pub(super) fn detect(
    mut detector: Box<dyn VoiceDetector>,
    chunks: std_mpsc::Receiver<(Instant, Vec<f32>)>,
    speaker: Speaker,
    session_start: Instant,
    first_seq: u64,
    speaking: &AtomicBool,
    tx: &mpsc::UnboundedSender<SourceEvent>,
) {
    let mut timeline = Timeline::new(session_start.elapsed().as_millis() as u64);
    let mut growing = Growing { next_seq: first_seq, ..Growing::default() };
    let emit = |detector: &mut dyn VoiceDetector, timeline: &Timeline, growing: &mut Growing| {
        let mut ended = false;
        while let Some(speech) = detector.pop() {
            let start_ms = timeline.ms_at(speech.start_sample);
            let end_ms = timeline.ms_at(speech.start_sample + speech.samples.len() as u64);
            let seq = growing.end();
            let _ = tx.send(SourceEvent::Ended { speaker, seq, start_ms, end_ms, samples: speech.samples });
            ended = true;
        }
        ended
    };
    loop {
        let (arrived, chunk) = match chunks.recv_timeout(Duration::from_millis(DELIVERY_GAP_MS)) {
            Ok(next) => next,
            // A tap goes quiet when nothing plays, so speech that just stopped would never end: the silence it
            // stopped into is fed instead.
            Err(RecvTimeoutError::Timeout) if growing.current.is_some() || detector.speaking() => {
                (Instant::now(), vec![0.0; (GAP_PAD_MS * SAMPLE_RATE as u64 / 1000) as usize])
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        let pad = timeline.chunk(chunk.len(), arrived.saturating_duration_since(session_start).as_millis() as u64);
        let mut block = vec![0.0; pad];
        block.extend_from_slice(&chunk);
        detector.accept(&block);
        let now_speaking = detector.speaking();
        speaking.store(now_speaking, Ordering::Relaxed);
        let ended = emit(detector.as_mut(), &timeline, &mut growing);
        match growing.feed(&block, now_speaking, ended, timeline.fed) {
            Fed::Grown(seq, start, samples) => {
                let start_ms = timeline.ms_at(start);
                let _ = tx.send(SourceEvent::Partial { speaker, seq, start_ms, samples });
            }
            Fed::Dropped(seq) => {
                let _ = tx.send(SourceEvent::Dropped { speaker, seq });
            }
            Fed::Nothing => {}
        }
    }
    detector.flush();
    speaking.store(false, Ordering::Relaxed);
    emit(detector.as_mut(), &timeline, &mut growing);
    if let Some(seq) = growing.current.take().map(|c| c.seq) {
        let _ = tx.send(SourceEvent::Dropped { speaker, seq });
    }
}

struct Current {
    seq: u64,
    /// Sample position of the first sample.
    start: u64,
    samples: Vec<f32>,
    /// Length of `samples` when they were last handed over.
    sent: usize,
}

/// The audio of the utterance going on, for partial recognition.
#[derive(Default)]
struct Growing {
    /// The last `PRE_ROLL` samples before speech started.
    recent: VecDeque<f32>,
    current: Option<Current>,
    next_seq: u64,
}

#[derive(Debug, PartialEq)]
enum Fed {
    Nothing,
    /// (seq, first sample position, samples)
    Grown(u64, u64, Vec<f32>),
    Dropped(u64),
}

impl Growing {
    /// The detector ended an utterance: it is the one going on, or a new one if speech was never seen at block
    /// granularity. Returns its number.
    fn end(&mut self) -> u64 {
        match self.current.take() {
            Some(c) => c.seq,
            None => self.take_seq(),
        }
    }

    fn take_seq(&mut self) -> u64 {
        self.next_seq += 1;
        self.next_seq
    }

    /// `ended` says the detector ended an utterance with this block (already taken by [`Self::end`]); `fed` is the
    /// sample position after `block`.
    fn feed(&mut self, block: &[f32], speaking: bool, ended: bool, fed: u64) -> Fed {
        let mut result = Fed::Nothing;
        if !speaking && let Some(c) = self.current.take() {
            result = Fed::Dropped(c.seq);
        }
        if speaking {
            // An utterance cut at its maximum length while speech goes on starts a fresh one right away.
            let fresh = ended || self.current.is_none();
            if fresh {
                let mut samples: Vec<f32> = if ended { Vec::new() } else { self.recent.iter().copied().collect() };
                samples.extend_from_slice(block);
                let (seq, start) = (self.take_seq(), fed - samples.len() as u64);
                self.current = Some(Current { seq, start, samples, sent: 0 });
            } else if let Some(c) = &mut self.current {
                c.samples.extend_from_slice(block);
            }
        }
        self.recent.extend(block);
        let excess = self.recent.len().saturating_sub(PRE_ROLL);
        self.recent.drain(..excess);
        if let Some(c) = &mut self.current
            && c.samples.len() >= c.sent + PARTIAL_STEP
        {
            c.sent = c.samples.len();
            return Fed::Grown(c.seq, c.start, c.samples.clone());
        }
        result
    }
}

/// A pause in delivery at least this long is a gap in the audio, not jitter.
const DELIVERY_GAP_MS: u64 = 500;
/// Silence fed at a gap, enough for the detector to end an utterance cut off by it.
const GAP_PAD_MS: u64 = 1_000;

/// Maps a detector's sample positions to session time. A system audio tap delivers nothing while nothing plays, so
/// the samples a detector has seen fall behind the clock; every gap adds an anchor instead of feeding the detector
/// the missing silence, which would cost an hour of inference for an hour of quiet.
struct Timeline {
    /// (sample position, session ms), ascending.
    anchors: Vec<(u64, u64)>,
    fed: u64,
}

impl Timeline {
    fn new(offset_ms: u64) -> Self {
        Self { anchors: vec![(0, offset_ms)], fed: 0 }
    }

    fn ms_at(&self, sample: u64) -> u64 {
        let rate = SAMPLE_RATE as u64;
        let (s, ms) = *self.anchors.iter().rev().find(|(s, _)| *s <= sample).expect("the first anchor is at 0");
        ms + (sample - s) * 1000 / rate
    }

    /// A chunk of `len` samples whose last sample arrived at session time `now_ms`. Returns how many silent samples
    /// to feed before it.
    fn chunk(&mut self, len: usize, now_ms: u64) -> usize {
        let rate = SAMPLE_RATE as u64;
        let start_ms = now_ms.saturating_sub(len as u64 * 1000 / rate);
        let gap = start_ms.saturating_sub(self.ms_at(self.fed));
        let mut pad = 0;
        if gap >= DELIVERY_GAP_MS {
            pad = (gap.min(GAP_PAD_MS) * rate / 1000) as usize;
            self.fed += pad as u64;
            self.anchors.push((self.fed, start_ms));
        }
        self.fed += len as u64;
        pad
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utterance_times_follow_the_clock_across_gaps_in_delivery() {
        let mut t = Timeline::new(1_000);
        // Steady delivery: 100 ms blocks, no padding, samples and clock agree.
        assert_eq!(t.chunk(1_600, 1_100), 0);
        assert_eq!(t.chunk(1_600, 1_200), 0);
        assert_eq!(t.ms_at(3_200), 1_200);
        // Nothing played for 10 s: one second of silence ends any utterance, and later samples map to the clock.
        assert_eq!(t.chunk(1_600, 11_300), 16_000);
        assert_eq!(t.ms_at(3_200 + 16_000), 11_200);
        assert_eq!(t.ms_at(3_200 + 16_000 + 1_600), 11_300);
        // Jitter below the gap threshold changes nothing.
        assert_eq!(t.chunk(1_600, 11_700), 0);
    }

    #[test]
    fn speech_going_on_is_handed_over_each_second_under_one_number_until_it_ends() {
        let mut g = Growing::default();
        let second = SAMPLE_RATE as usize;
        let block = vec![0.1; second / 10];
        let mut fed = 0;
        let mut feed = |g: &mut Growing, speaking: bool, ended: bool| {
            fed += block.len() as u64;
            g.feed(&block, speaking, ended, fed)
        };
        for _ in 0..10 {
            assert_eq!(feed(&mut g, false, false), Fed::Nothing);
        }
        let mut handed = Vec::new();
        for _ in 0..20 {
            if let Fed::Grown(seq, start, samples) = feed(&mut g, true, false) {
                handed.push((seq, start, samples.len()));
            }
        }
        // The pre-roll counts towards the first second.
        let start = (second - PRE_ROLL) as u64;
        assert_eq!(handed, vec![(1, start, second), (1, start, 2 * second)]);

        // Cut at its maximum length while speech goes on: the next utterance starts without a pre-roll.
        assert_eq!(g.end(), 1);
        assert_eq!(feed(&mut g, true, true), Fed::Nothing);
        assert_eq!(g.current.as_ref().map(|c| c.seq), Some(2));
        // Speech that stops without becoming an utterance is dropped.
        assert_eq!(feed(&mut g, false, false), Fed::Dropped(2));
        assert!(g.current.is_none());
        // An utterance the detector ends without speech ever being seen gets a number of its own.
        assert_eq!(g.end(), 3);
    }
}
