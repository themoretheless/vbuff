use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("invalid sync data: {0}")]
    Invalid(String),
    #[error("cryptographic operation failed")]
    Crypto,
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("compression failed: {0}")]
    Compression(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SyncError>;
