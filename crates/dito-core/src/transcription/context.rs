//! The recording as the rest of the app reads it: which meeting is recording, whether it is paused, and the latest
//! transcript text available, closed paragraphs and the words still being heard. Chat attachments and the live
//! meeting notes read the meeting only through this module, so both see exactly what the recording page shows
//! (`design:tools/recording/live-notes`).

use super::session::{SessionState, TranscriptionStatus};
use super::{LiveText, Speaker};
use crate::store::{ParagraphState, Store, StoreError};

/// What the session is doing with the meeting that is open for recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingPhase {
    Recording,
    Paused,
    /// The audio is closed and the last utterances are being finished.
    Finishing,
}

/// The meeting a session is writing, as shown on the sidebar, in Chat's way back and on a live attachment. `None`
/// while idle or starting: a recording that has not opened its meeting has nothing to return to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveRecording {
    pub id: i64,
    pub phase: RecordingPhase,
}

impl ActiveRecording {
    pub fn of(status: &TranscriptionStatus) -> Option<Self> {
        match &status.state {
            SessionState::Recording { id, segment_start, .. } => Some(Self {
                id: *id,
                phase: if segment_start.is_some() { RecordingPhase::Recording } else { RecordingPhase::Paused },
            }),
            SessionState::Finishing { id } => Some(Self { id: *id, phase: RecordingPhase::Finishing }),
            SessionState::Idle | SessionState::Starting => None,
        }
    }

    /// Whether Chat offers the way back: while recording and while paused, not while the session winds down.
    pub fn can_return(self) -> bool {
        self.phase != RecordingPhase::Finishing
    }
}

/// One thing a speaker said, on the meeting's own clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utterance {
    pub speaker: Speaker,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// The names speakers carry in text handed to a model, in the interface language.
#[derive(Debug, Clone, Copy)]
pub struct SpeakerNames<'a> {
    pub me: &'a str,
    pub others: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingContext {
    pub id: i64,
    pub title: String,
    pub started_ts: i64,
    pub duration_ms: Option<u64>,
    /// Speakers are named only when both sources were recorded.
    pub both_sources: bool,
    /// The session is still writing this meeting, so more text may follow.
    pub live: bool,
    pub utterances: Vec<Utterance>,
}

impl MeetingContext {
    /// The latest text available for meeting `id`. For the meeting a session is writing, that is the closed
    /// paragraphs the recording page shows plus the words still being heard; otherwise it is the stored transcript.
    /// `None` when the meeting is gone.
    pub fn read(store: &Store, status: &TranscriptionStatus, id: i64) -> Result<Option<Self>, StoreError> {
        let Some(transcript) = store.get_transcript(id)? else { return Ok(None) };
        let live = status.state.transcript_id() == Some(id);
        let stored = transcript.paragraphs.iter().filter(|p| p.state != ParagraphState::Listening);
        let closed: Vec<_> = if live { stored.take(status.closed).collect() } else { stored.collect() };
        let mut utterances: Vec<Utterance> = closed
            .into_iter()
            .map(|p| Utterance {
                speaker: p.speaker,
                start_ms: p.start_ms,
                end_ms: p.end_ms.max(p.start_ms),
                text: p.text.trim().to_owned(),
            })
            .filter(|u| !u.text.is_empty())
            .collect();
        if live {
            utterances.extend(status.live.iter().filter_map(heard));
        }
        Ok(Some(Self {
            id,
            title: transcript.summary.title.trim().to_owned(),
            started_ts: transcript.summary.started_ts,
            duration_ms: transcript.summary.length_ms,
            both_sources: transcript.summary.sources.both(),
            live,
            utterances,
        }))
    }

    /// A meeting with nothing in it, for a read that failed.
    pub fn empty(id: i64) -> Self {
        Self {
            id,
            title: String::new(),
            started_ts: 0,
            duration_ms: None,
            both_sources: false,
            live: false,
            utterances: Vec::new(),
        }
    }

    /// The end of the last utterance: how far the text reaches.
    pub fn covered_ms(&self) -> u64 {
        self.utterances.iter().map(|u| u.end_ms).max().unwrap_or(0)
    }

    pub fn chars(&self) -> usize {
        self.utterances.iter().map(|u| u.text.chars().count()).sum()
    }

    /// The unix second an offset on the meeting's clock falls on.
    pub fn wall_ts(&self, ms: u64) -> i64 {
        self.started_ts + (ms / 1000) as i64
    }

    /// One block per utterance, the speaker named only when both sources were recorded and a time mark in front when
    /// `timed`, which lets a model cite when something was said. `local_offset_secs` places the mark in local time.
    pub fn text(&self, names: SpeakerNames<'_>, timed: bool, local_offset_secs: i32) -> String {
        self.utterances
            .iter()
            .map(|u| {
                let mut line = String::new();
                if timed {
                    line.push_str(&format!("[{}] ", hhmm(self.wall_ts(u.start_ms), local_offset_secs)));
                }
                if self.both_sources {
                    line.push_str(if u.speaker == Speaker::Me { names.me } else { names.others });
                    line.push_str(": ");
                }
                line.push_str(&u.text);
                line
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

/// The words still being heard for one speaker, as one utterance that ends where it starts.
fn heard(live: &LiveText) -> Option<Utterance> {
    let text = format!("{}{}", live.settled, live.unsettled).trim().to_owned();
    (!text.is_empty()).then_some(Utterance {
        speaker: live.speaker,
        start_ms: live.start_ms,
        end_ms: live.start_ms,
        text,
    })
}

pub fn hhmm(ts: i64, offset_secs: i32) -> String {
    let s = (ts + offset_secs as i64).rem_euclid(86_400);
    format!("{:02}:{:02}", s / 3600, (s % 3600) / 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Sources;

    const NAMES: SpeakerNames<'static> = SpeakerNames { me: "自分", others: "相手" };

    fn recording(id: i64, segment_start: Option<i64>) -> TranscriptionStatus {
        TranscriptionStatus {
            state: SessionState::Recording {
                id,
                started_ts: 0,
                sources: Sources::default(),
                recorded_ms: 0,
                segment_start,
            },
            ..Default::default()
        }
    }

    #[test]
    fn the_active_recording_follows_the_session_state() {
        assert_eq!(ActiveRecording::of(&TranscriptionStatus::default()), None);
        let mut status = recording(7, Some(1));
        let active = ActiveRecording::of(&status).unwrap();
        assert_eq!((active.id, active.phase, active.can_return()), (7, RecordingPhase::Recording, true));
        status = recording(7, None);
        assert_eq!(ActiveRecording::of(&status).unwrap().phase, RecordingPhase::Paused);
        assert!(ActiveRecording::of(&status).unwrap().can_return(), "a paused recording still offers the way back");
        status.state = SessionState::Finishing { id: 7 };
        assert!(!ActiveRecording::of(&status).unwrap().can_return());
        status.state = SessionState::Starting;
        assert_eq!(ActiveRecording::of(&status), None);
    }

    #[test]
    fn a_live_meeting_reads_closed_paragraphs_then_the_words_still_being_heard() {
        let store = Store::open_in_memory().unwrap();
        let id = store.create_transcript(3_600, Sources { system: true, microphone: true }).unwrap();
        let first = store.add_paragraph(id, Speaker::Others, 0, 4_000, "金曜に再テストします。").unwrap();
        store.finish_paragraph(first, "金曜に再テストします。", None).unwrap();
        let later = store.add_paragraph(id, Speaker::Me, 5_000, 6_000, "closed after the last status").unwrap();
        store.set_paragraph_polishing(later).unwrap();
        store.add_paragraph(id, Speaker::Others, 9_000, 9_500, "listening, stored for a crash").unwrap();

        let mut status = recording(id, Some(1));
        status.closed = 1;
        status.live = vec![LiveText {
            speaker: Speaker::Me,
            start_ms: 61_000,
            settled: "はい。".into(),
            unsettled: "検証環境に".into(),
        }];
        let context = MeetingContext::read(&store, &status, id).unwrap().unwrap();
        assert!(context.live && context.both_sources);
        let texts: Vec<_> = context.utterances.iter().map(|u| u.text.as_str()).collect();
        assert_eq!(texts, ["金曜に再テストします。", "はい。検証環境に"], "a paragraph is shown in one place only");
        assert_eq!(context.covered_ms(), 61_000);
        assert_eq!(
            context.text(NAMES, true, 0),
            "[01:00] 相手: 金曜に再テストします。\n\n[01:01] 自分: はい。検証環境に"
        );
        assert_eq!(context.text(NAMES, false, 9 * 3600), "相手: 金曜に再テストします。\n\n自分: はい。検証環境に");
    }

    #[test]
    fn a_finished_meeting_reads_its_stored_paragraphs_and_one_source_names_no_speaker() {
        let store = Store::open_in_memory().unwrap();
        let id = store.create_transcript(0, Sources::default()).unwrap();
        for (i, text) in ["一つ目。", "  ", "二つ目。"].into_iter().enumerate() {
            let at = i as u64 * 1_000;
            let p = store.add_paragraph(id, Speaker::Others, at, at + 500, text).unwrap();
            store.finish_paragraph(p, text, None).unwrap();
        }
        let context = MeetingContext::read(&store, &TranscriptionStatus::default(), id).unwrap().unwrap();
        assert!(!context.live);
        assert_eq!(context.text(NAMES, false, 0), "一つ目。\n\n二つ目。");
        assert_eq!(MeetingContext::read(&store, &TranscriptionStatus::default(), id + 1).unwrap(), None);
    }
}
