use std::path::PathBuf;

use crate::catalog::Runtime;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("no inference runtime for {0:?} is built into this app")]
    NotBuiltIn(Runtime),
    #[error("{name}: the {runtime:?} runtime is not compiled into this build")]
    NotCompiled { name: String, runtime: Runtime },
    #[error("{} is missing", .0.display())]
    MissingFile(PathBuf),
    #[error("no matching file in {}", .0.display())]
    NoMatchingFile(PathBuf),
    #[error("{}: {source}", dir.display())]
    ReadDir { dir: PathBuf, source: std::io::Error },
    #[error("{0} poisoned")]
    LockPoisoned(&'static str),
    #[error("mmproj file does not support audio")]
    AudioUnsupported,
    #[error("inference was cancelled")]
    Cancelled,
    #[error("output was cut off at the token limit")]
    Truncated,
    #[error("{0}")]
    Engine(String),
}

impl RuntimeError {
    pub fn engine(error: impl std::fmt::Display) -> Self {
        Self::Engine(error.to_string())
    }
}
