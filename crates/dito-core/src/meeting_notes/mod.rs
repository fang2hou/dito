//! Live notes for the meeting being recorded (`design:tools/recording/live-notes`): a hint naming the topic
//! discussed now, and a memo of the whole meeting with what was decided, what is to be done, what is open and
//! how the discussion went, written by the chat model while the recording goes on, in the meeting's language.
//!
//! Four parts, each testable alone. `schedule` decides when a refresh is due (the first one at once, then about
//! once a minute while new text arrives, none while paused, one more when the meeting ends). `lang` decides the
//! meeting's language from the transcript. `prompt` builds the fixed-scaffold requests and reads the replies back
//! (ADR-0012). `runner` is the actor that reads the meeting through
//! [`MeetingContext`](crate::transcription::MeetingContext), runs one request at a time and publishes the result.
//! Notes live in memory only and go to the chat model in use, local or cloud (ADR-0008).

mod lang;
mod prompt;
mod runner;
mod schedule;

pub use lang::{Judge, LangChoice, MeetingLang, classify, language_sample, meeting_lang};
pub use prompt::{Budget, Hint, Item, MAX_TRAIL, Memo, Notes, ReplyError};
pub use runner::{
    Cadence, ModelFuture, NotesDeps, NotesHandle, NotesModel, NotesModelSource, NotesPhase, NotesView, Unavailable,
    spawn,
};
pub use schedule::{Poll, REFRESH_EVERY, Scheduler};

/// The memo takes in speech once it is older than this.
pub const RECENT_WINDOW_MS: u64 = 5 * 60 * 1000;
