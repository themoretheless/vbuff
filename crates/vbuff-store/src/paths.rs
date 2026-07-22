//! Resolution of the default on-disk database location.

use std::path::PathBuf;

use crate::{Result, StoreError};

/// The default database path: `<data_dir>/vbuff/history.db`.
pub fn default_db_path() -> Result<PathBuf> {
    let dir = dirs_data_dir().ok_or(StoreError::NoDataDir)?;
    Ok(dir.join("vbuff").join("history.db"))
}

/// Resolve the platform data directory.
fn dirs_data_dir() -> Option<PathBuf> {
    // Avoid a hard `dirs` dependency in this crate by re-implementing the
    // small bit we need via std + env fallbacks. The app crate uses `dirs`
    // directly; here we keep the store dependency-light.
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|h| h.join("Library").join("Application Support"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            return Some(PathBuf::from(xdg));
        }
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|h| h.join(".local").join("share"))
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}
