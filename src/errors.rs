use thiserror::Error;

#[derive(Error, Debug)]
pub enum GpmError {
    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Package error: {0}")]
    PackageError(String),

    #[error("Package not found: {0}")]
    PackageNotFoundError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Self update error: {0}")]
    SelfUpdate(#[from] self_update::errors::Error),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

pub type Result<T> = std::result::Result<T, GpmError>;
