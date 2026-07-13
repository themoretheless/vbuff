//! Error type for the store.

use thiserror::Error;

/// Errors that can arise from store operations.
#[derive(Debug, Error)]
pub enum StoreError {
    /// Underlying SQLite error.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Filesystem error creating the data directory.
    #[error("io error: {0}")]
    Io(#[source] std::io::Error),

    /// JSON (de)serialization of the flavor blob failed.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// The platform data directory could not be determined.
    #[error("could not determine data directory")]
    NoDataDir,

    /// A stored row was malformed.
    #[error("corrupt store row: {0}")]
    Corrupt(String),

    /// A transactional mutation referred to a clip that does not exist.
    #[error("clip not found: {0}")]
    ClipNotFound(String),

    /// A migration preflight, verification, or rollback failed.
    #[error("migration error: {0}")]
    Migration(String),

    /// Privacy maintenance could not finish after a logical mutation.
    #[error("store maintenance error: {0}")]
    Maintenance(String),
}
