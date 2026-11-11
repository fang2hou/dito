#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    KeyDown,
    KeyUp,
    Cancel,
    Finish,
    Stop,
    Retry,
    Dismiss,
    /// Keep an error visible while its controls or explanation are being inspected.
    ErrorHover(bool),
}
