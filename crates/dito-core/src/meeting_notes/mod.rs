//! Live notes for the meeting being recorded (`design:tools/recording/live-notes`): the current topic, what was said
//! in the last five minutes, and a few points that fold everything earlier, written by the local chat model while
//! the recording goes on.
//!
//! Three parts, each testable alone. `schedule` decides when a refresh is due (the first one at once, then about
//! once a minute while new text arrives, none while paused, one more when the meeting ends). `prompt` builds the
//! fixed-scaffold requests and reads the replies back (ADR-0012). `runner` is the actor that reads the meeting
//! through [`MeetingContext`](crate::transcription::MeetingContext), runs one request at a time and publishes the
//! result. Notes live in memory only and go to the local model only (ADR-0002, ADR-0008).

mod prompt;
mod runner;
mod schedule;

pub use prompt::{Note, Notes};
pub use runner::{
    Cadence, ModelFuture, NotesDeps, NotesHandle, NotesModel, NotesModelSource, NotesPhase, NotesView, Unavailable,
    spawn,
};
pub use schedule::{Poll, REFRESH_EVERY, Scheduler};

/// How much of the newest speech the notes keep in detail.
pub const RECENT_WINDOW_MS: u64 = 5 * 60 * 1000;

/// Older speech is folded into this many points, every time a window's worth of it has aged out.
pub const EARLIER_POINTS: std::ops::RangeInclusive<usize> = 4..=5;
