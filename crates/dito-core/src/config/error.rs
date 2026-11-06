use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {}", path.display())]
    Read { path: PathBuf, source: std::io::Error },
    #[error("invalid config: {}", path.display())]
    Invalid { path: PathBuf, source: toml::de::Error },
    #[error(transparent)]
    Write(#[from] std::io::Error),
    #[error(transparent)]
    Encode(#[from] toml::ser::Error),
}
