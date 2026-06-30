//! Error type for platform backends.

use thiserror::Error;

/// Errors raised by platform backends.
#[derive(Debug, Error)]
pub enum PlatformError {
    /// The clipboard could not be read or written.
    #[error("clipboard error: {0}")]
    Clipboard(String),

    /// Registering or unregistering a hotkey failed.
    #[error("hotkey error: {0}")]
    Hotkey(String),

    /// Synthesizing a paste keystroke failed.
    #[error("paste error: {0}")]
    Paste(String),

    /// A key combo string could not be parsed.
    #[error("invalid hotkey combo: {0}")]
    BadCombo(String),

    /// The clipboard held no readable content.
    #[error("clipboard empty")]
    Empty,
}
