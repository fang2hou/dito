#[derive(Debug, thiserror::Error)]
pub enum InsertError {
    #[error("the clipboard did not accept the text")]
    Clipboard,
    #[error("could not send Cmd+V")]
    PasteKeystroke,
}
