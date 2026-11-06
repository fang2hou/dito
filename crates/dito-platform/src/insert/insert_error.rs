#[derive(Debug, thiserror::Error)]
pub enum InsertError {
    #[error(transparent)]
    Clipboard(#[from] arboard::Error),
    #[error("could not send Cmd+V")]
    PasteKeystroke,
}
