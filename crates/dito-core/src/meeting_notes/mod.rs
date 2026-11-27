//! Live notes for the meeting being recorded (`design:tools/recording/live-notes`): a hint naming the topic
//! discussed now, and a memo of the whole meeting with what was decided, what is to be done, what is open and
//! how the discussion went, written by the chat model while the recording goes on, in the meeting's language.
//!
//! The first hint is immediate. While text arrives, hints use the last minute and run about every 30 seconds,
//! less often for slow models. The memo takes in new speech about every five minutes of recording time, in
//! chunks when needed. The runner awaits each request and only advances coverage on success. A successful
//! memo retires the hints it covers. Pausing allows only the first hint, and ending allows one final memo.
//! `lang` chooses the meeting language, `prompt` builds the two requests and reads their replies (ADR-0012),
//! and the runner reads the transcript through [`MeetingContext`](crate::transcription::MeetingContext).
//! Notes live in memory only and go to the chat model in use, local or cloud (ADR-0008).

mod lang;
mod prompt;
mod runner;
mod schedule;

pub use lang::{Judge, LangChoice, MeetingLang, classify, language_sample, meeting_lang};
pub use prompt::{Budget, Hint, Item, MAX_TRAIL, Memo, Notes, ReplyError};
pub use runner::{
    Cadence, ModelFuture, NotesDeps, NotesHandle, NotesModel, NotesModelSource, NotesPhase, NotesView, Stale,
    Unavailable, spawn,
};
