use crate::intent::Intent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    KeyDown,
    /// [`Self::KeyDown`] when the command shortcut is the dictation shortcut plus modifiers, so this press may turn
    /// out to be its first half: the recording starts at once, but the capsule waits [`super::CHORD_HOLD`] for the
    /// handover before it appears.
    KeyDownMayHandOver,
    KeyUp,
    Cancel,
    Finish,
    Stop,
    Retry,
    /// Closes an outcome, an error or a command window.
    Dismiss,
    /// Keep an error or the not-recognized notice visible while it is being inspected.
    ErrorHover(bool),
    /// The command shortcut. Toggle: a press of either shortcut ends a recording of either kind. Hold: a recording
    /// ends when the shortcut that started it is released.
    CommandKeyDown,
    CommandKeyUp,
    /// The command shortcut's keys completed the dictation shortcut's press that started this recording: it carries
    /// on as a command run, with no second press. Ignored unless a recording is running.
    KeyToCommand,
    /// The reverse, for a dictation shortcut that is the command shortcut plus modifiers.
    CommandToKey,
    /// An ordinary key followed a lone modifier right after it went down: that was typing. Drops the recording the
    /// press started; it never touches a run the press only ended.
    ChordAbort,
    /// The polish button of the command capsule: polish the selection at once, without routing.
    PolishSelection,
    /// The keyboard button of the command capsule: type the instruction instead of speaking it.
    TypeInstruction,
    SubmitInstruction(String),
    /// The text a tool asked for because nothing was selected.
    SubmitText(String),
    /// A button of the choice window.
    Choose(Intent),
    /// The result window's buttons, with the window's current, possibly edited, text.
    Replace(String),
    CopyResult(String),
}
