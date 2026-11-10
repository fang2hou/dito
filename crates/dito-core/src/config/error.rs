use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {}", path.display())]
    Read { path: PathBuf, source: std::io::Error },
    #[error("invalid config: {}", path.display())]
    Invalid { path: PathBuf, source: toml::de::Error },
    #[error(
        "{} was saved by a newer Dito (schema version {found}, this build supports up to {supported})",
        path.display()
    )]
    NewerSchema { path: PathBuf, found: u32, supported: u32 },
    #[error(transparent)]
    Write(#[from] std::io::Error),
    #[error(transparent)]
    Encode(#[from] toml::ser::Error),
}
