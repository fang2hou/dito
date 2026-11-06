use std::path::PathBuf;

/// The user's Downloads folder, where save dialogs start.
pub fn downloads_dir() -> PathBuf {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Downloads")).unwrap_or_else(|| ".".into())
}
