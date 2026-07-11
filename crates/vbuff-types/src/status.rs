//! Serializable runtime status contracts shared by capture, GUI, tray, and IPC.

use serde::{Deserialize, Serialize};

/// Observable health of the resident clipboard-capture path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureHealth {
    /// The worker has not opened its clipboard backend yet.
    #[default]
    Starting,
    /// Clipboard reads are available and persistence is healthy.
    Watching,
    /// The clipboard backend could not be created; capture is not running.
    ClipboardUnavailable,
    /// A clipboard read failed; the worker will keep retrying.
    ClipboardReadError,
    /// A captured clip could not be persisted; the same clip will be retried.
    StorageError,
    /// The worker stopped publishing heartbeats within the watchdog budget.
    Stalled,
}

impl CaptureHealth {
    /// Stable user-facing label shared by popup and tray.
    pub fn label(self) -> &'static str {
        match self {
            Self::Starting => "Capture starting",
            Self::Watching => "Capture active",
            Self::ClipboardUnavailable => "Clipboard unavailable",
            Self::ClipboardReadError => "Clipboard read issue",
            Self::StorageError => "History write issue",
            Self::Stalled => "Capture stalled",
        }
    }
}

/// Severity for a redacted, user-visible command result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

/// Last command outcome shown without exposing clipboard payloads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandNotice {
    pub level: NoticeLevel,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_and_notice_roundtrip_for_future_ipc() {
        let health = CaptureHealth::StorageError;
        let health_json = serde_json::to_string(&health).unwrap();
        assert_eq!(health_json, r#""storage_error""#);
        assert_eq!(
            serde_json::from_str::<CaptureHealth>(&health_json).unwrap(),
            health
        );
        assert_eq!(
            serde_json::to_string(&CaptureHealth::Stalled).unwrap(),
            r#""stalled""#
        );

        let notice = CommandNotice {
            level: NoticeLevel::Warning,
            message: "Copy-only mode".into(),
        };
        let notice_json = serde_json::to_string(&notice).unwrap();
        assert_eq!(
            serde_json::from_str::<CommandNotice>(&notice_json).unwrap(),
            notice
        );
    }
}
