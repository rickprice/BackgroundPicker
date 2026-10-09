#[derive(Debug, thiserror::Error)]
pub enum BackgroundPickerError {
    #[error("Failed to create thread pool: {0}")]
    ThreadPoolCreation(#[from] rayon::ThreadPoolBuildError),

    #[error("Failed to create thumbnail cache directory: {0}")]
    CacheDirectoryCreation(#[from] std::io::Error),

    #[error("Failed to save selected image path: {0}")]
    SaveSelectedImage(std::io::Error),

    #[error("Command execution failed: {0}")]
    CommandExecution(String),
}

pub type Result<T> = std::result::Result<T, BackgroundPickerError>;
