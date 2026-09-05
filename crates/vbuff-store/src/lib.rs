//! History storage with interchangeable build-time DuckDB and SQLite backends.
#![forbid(unsafe_code)]

#[cfg(all(feature = "duckdb", feature = "sqlite"))]
compile_error!("Select one store backend: use --no-default-features --features sqlite for SQLite");

#[cfg(feature = "duckdb")]
include!("duckdb_backend.rs");
#[cfg(not(feature = "duckdb"))]
include!("sqlite_backend.rs");

/// The engine used by this build. No database format is guessed from file extensions.
pub const BACKEND: &str = if cfg!(feature = "duckdb") {
    "duckdb"
} else {
    "sqlite"
};

mod tags;

mod ttl;
