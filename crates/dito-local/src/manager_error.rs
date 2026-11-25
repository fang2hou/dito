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
    #[error("the new models folder is inside a model's folder")]
    TargetInsideModel,
    #[error("{} already exists in the new models folder", .0.display())]
    TargetExists(std::path::PathBuf),
    #[error("not enough free space for the models: {needed} bytes needed, {free} free")]
    InsufficientSpace { needed: u64, free: u64 },
}
