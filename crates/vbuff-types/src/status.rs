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
    /// Active read/write/restore probe did not complete coherently.
    SelfTestFailed,
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
            Self::SelfTestFailed => "Capture self-test failed",
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

/// Content-free capture accounting shown in the popup and future IPC clients.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureSessionStats {
    pub captured: u64,
    pub intentionally_skipped: u64,
    pub lost: u64,
}

/// Coarse security state suitable for compact UI and IPC surfaces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityPostureLevel {
    Protected,
    #[default]
    Partial,
    Blocked,
}

impl SecurityPostureLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Protected => "Security protected",
            Self::Partial => "Security partial",
            Self::Blocked => "Security blocked",
        }
    }
}

/// Content-free summary derived from the platform's detailed capability report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityPostureSummary {
    pub level: SecurityPostureLevel,
    pub active: u16,
    pub degraded: u16,
    pub unavailable: u16,
    pub strict_mode: bool,
}

/// Detailed state of one platform capability shown on the trust surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityViewLevel {
    Active,
    Degraded,
    Unavailable,
    NotApplicable,
}

impl CapabilityViewLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Degraded => "Degraded",
            Self::Unavailable => "Unavailable",
            Self::NotApplicable => "Not applicable",
        }
    }
}

/// Content-free capability evidence suitable for GUI and future IPC clients.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityView {
    pub feature: String,
    pub level: CapabilityViewLevel,
    pub detail: String,
}

/// Classification of one content-free capture decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyDecisionLevel {
    Captured,
    Skipped,
    Lost,
}

impl PrivacyDecisionLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Captured => "Captured",
            Self::Skipped => "Skipped",
            Self::Lost => "Lost",
        }
    }
}

/// One entry from the bounded privacy ledger. Clipboard content is never stored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivacyEventSummary {
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub count: u64,
    pub decision: PrivacyDecisionLevel,
    pub reason: String,
}

/// Bounded, tamper-evident privacy ledger projection for the trust UI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivacyLedgerSummary {
    pub chain_valid: bool,
    pub head_hash_prefix: String,
    pub recent: Vec<PrivacyEventSummary>,
}

impl Default for PrivacyLedgerSummary {
    fn default() -> Self {
        Self {
            chain_valid: true,
            head_hash_prefix: "000000000000".into(),
            recent: Vec::new(),
        }
    }
}

/// Measurement state used by every release SLO row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SloMetricState {
    Met,
    Breached,
    Unknown,
}

impl SloMetricState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Met => "Met",
            Self::Breached => "Breached",
            Self::Unknown => "Unknown",
        }
    }
}

/// User-visible SLO evidence. Missing measurements stay explicitly unknown.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SloStatusSummary {
    pub zero_loss: SloMetricState,
    pub search_latency: SloMetricState,
    pub idle_cpu: SloMetricState,
    pub login_ready: SloMetricState,
}

impl Default for SloStatusSummary {
    fn default() -> Self {
        Self {
            zero_loss: SloMetricState::Unknown,
            search_latency: SloMetricState::Unknown,
            idle_cpu: SloMetricState::Unknown,
            login_ready: SloMetricState::Unknown,
        }
    }
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

        let posture = SecurityPostureSummary {
            level: SecurityPostureLevel::Blocked,
            active: 2,
            degraded: 1,
            unavailable: 3,
            strict_mode: true,
        };
        let posture_json = serde_json::to_string(&posture).unwrap();
        assert_eq!(
            serde_json::from_str::<SecurityPostureSummary>(&posture_json).unwrap(),
            posture
        );
        assert_eq!(posture.level.label(), "Security blocked");

        let ledger = PrivacyLedgerSummary::default();
        assert!(ledger.chain_valid);
        assert_eq!(
            SloStatusSummary::default().search_latency,
            SloMetricState::Unknown
        );
        assert_eq!(
            SloStatusSummary::default().zero_loss,
            SloMetricState::Unknown
        );
    }
}
