#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// The recording shortcut went down / up.
    KeyDown,
    KeyUp,
    /// Esc or the × button.
    Cancel,
    /// The ✓ button.
    Finish,
    /// "中止" while transcribing or polishing.
    Stop,
    /// Retry button on an error.
    Retry,
    Dismiss,
    /// Keep an error visible while its controls or explanation are being inspected.
    ErrorHover(bool),
}
