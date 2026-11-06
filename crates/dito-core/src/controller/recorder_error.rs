#[derive(Debug, thiserror::Error)]
pub enum RecorderError {
    #[error("no input device")]
    NoInputDevice,
    #[error("timed out opening the input device")]
    OpenTimeout,
    #[error("unsupported sample format {0}")]
    UnsupportedSampleFormat(String),
    #[error("{0}")]
    Device(String),
    #[error(transparent)]
    Thread(#[from] std::io::Error),
}
