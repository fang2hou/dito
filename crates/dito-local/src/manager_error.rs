#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    #[error("unknown model {0}")]
    UnknownModel(String),
    #[error("download in progress")]
    DownloadInProgress,
    #[error("downloads are running")]
    DownloadsRunning,
    #[error("the models folder is being moved")]
    FolderMoving,
    #[error("disk: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not save the models folder: {0}")]
    Config(String),
}
