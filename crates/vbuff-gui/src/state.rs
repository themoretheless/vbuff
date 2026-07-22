//! Shared state and action types exchanged between the GUI and the app wiring.

use std::collections::HashSet;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vbuff_core::onboarding::DefaultProfile;
use vbuff_core::trust::PrivacyScore;
use vbuff_types::{
    CapabilityView, CaptureBudgetAlert, CaptureHealth, CapturePauseReason, CaptureSessionStats,
    Clip, ClipId, ClipboardHealthDigest, CommandNotice, NoticeLevel, PrivacyLedgerSummary,
    SecurityPostureSummary, SloStatusSummary,
};

use crate::experience::UiPreferences;

/// The live state the GUI renders. Owned behind a [`SharedState`] lock so the
/// background capture thread can push new clips while the GUI reads them.
#[derive(Default)]
pub struct AppState {
    /// The current clip list, already ordered (pinned first, then recency).
    pub clips: Arc<[Clip]>,
    /// True if clipboard capture is currently paused.
    pub paused: bool,
    /// Why capture is paused; `None` while capture is running.
    pub pause_reason: Option<CapturePauseReason>,
    /// Current health of the resident capture worker.
    pub capture_health: CaptureHealth,
    /// Content-free accounting for this resident-process session.
    pub capture_stats: CaptureSessionStats,
    /// A de-duplicated capture failure that links into the Trust surface.
    pub health_alert: Option<CaptureHealth>,
    /// A de-duplicated large-payload event that links into Settings.
    pub size_budget_alert: Option<CaptureBudgetAlert>,
    /// Content-free store-health snapshot refreshed outside the capture path.
    pub health_digest: ClipboardHealthDigest,
    /// Clips exempt from automatic lifecycle cleanup for this process session.
    pub session_protected: HashSet<ClipId>,
    /// Payloads held only in this resident process and never persisted.
    pub memory_only_clips: HashSet<ClipId>,
    /// Applied first-run profile, when one was chosen.
    pub default_profile: Option<DefaultProfile>,
    /// Capability-honest security state derived by the platform layer.
    pub security_posture: SecurityPostureSummary,
    /// Detailed capability evidence; no inferred green states.
    pub capabilities: Vec<CapabilityView>,
    /// Content-free, hash-chained capture decisions.
    pub privacy_ledger: PrivacyLedgerSummary,
    /// Content-free score derived from the effective local privacy settings.
    pub privacy_score: Option<PrivacyScore>,
    /// Release SLO status; unavailable measurements remain unknown.
    pub slo_status: SloStatusSummary,
    /// A recent privacy skip may be explicitly re-read from the live clipboard.
    pub recoverable_skip_until: Option<Instant>,
    /// Latest redacted command result, dismissible from the popup.
    pub notice: Option<CommandNotice>,
    /// Screen-reader live-region message. Content is intentionally generic.
    pub accessibility_announcement: Option<String>,
    /// Bumped for every live-region message, including repeated text.
    pub announcement_revision: u64,
    /// Resolved summon shortcut shown by the one-time coachmark.
    pub hotkey_label: Option<String>,
    /// Whether the native resident process is registered to start at login.
    pub launch_at_login: bool,
    /// True until the coachmark is explicitly dismissed.
    pub show_hotkey_coachmark: bool,
    /// Set to true by the wiring when the popup should be shown/focused.
    pub show_requested: bool,
    /// A monotonically increasing revision; bumped when `clips` changes so the
    /// GUI can cheaply detect updates.
    pub revision: u64,
}

impl AppState {
    /// Construct the initial state from the persisted history snapshot.
    pub fn with_clips(clips: Vec<Clip>) -> Self {
        Self {
            clips: Arc::from(clips),
            ..Default::default()
        }
    }

    /// Replace the clip list and bump the revision.
    pub fn set_clips(&mut self, clips: Vec<Clip>) {
        self.clips = Arc::from(clips);
        self.revision = self.revision.wrapping_add(1);
    }

    /// Request the popup be shown and focused on the next frame.
    pub fn request_show(&mut self) {
        self.show_requested = true;
    }

    /// Publish capture health, returning true only when it changed.
    pub fn set_capture_health(&mut self, health: CaptureHealth) -> bool {
        if self.capture_health == health {
            return false;
        }
        self.capture_health = health;
        if !matches!(health, CaptureHealth::Starting | CaptureHealth::Watching) {
            self.health_alert = Some(health);
            self.announce(format!("Capture alert: {}", health.label()));
        }
        true
    }

    pub fn set_size_budget_alert(&mut self, alert: CaptureBudgetAlert) -> bool {
        if self.size_budget_alert == Some(alert) {
            return false;
        }
        self.size_budget_alert = Some(alert);
        self.announce(alert.label());
        true
    }

    pub fn add_capture_stats(&mut self, captured: u64, skipped: u64, lost: u64) {
        self.capture_stats.captured = self.capture_stats.captured.saturating_add(captured);
        self.capture_stats.intentionally_skipped = self
            .capture_stats
            .intentionally_skipped
            .saturating_add(skipped);
        self.capture_stats.lost = self.capture_stats.lost.saturating_add(lost);
    }

    pub fn offer_skipped_recovery(&mut self, now: Instant, window: Duration) {
        self.recoverable_skip_until = now.checked_add(window);
    }

    pub fn clear_skipped_recovery(&mut self) {
        self.recoverable_skip_until = None;
    }

    pub fn skipped_recovery_available(&self, now: Instant) -> bool {
        self.recoverable_skip_until
            .is_some_and(|deadline| now <= deadline)
    }

    pub fn take_skipped_recovery(&mut self, now: Instant) -> bool {
        let available = self.skipped_recovery_available(now);
        self.clear_skipped_recovery();
        available
    }

    /// Replace the current command notice with a redacted message.
    pub fn set_notice(&mut self, level: NoticeLevel, message: impl Into<String>) {
        let message = message.into();
        self.notice = Some(CommandNotice {
            level,
            message: message.clone(),
        });
        self.announce(message);
    }

    pub fn clear_notice(&mut self) {
        self.notice = None;
    }

    pub fn announce(&mut self, message: impl Into<String>) {
        self.accessibility_announcement = Some(message.into());
        self.announcement_revision = self.announcement_revision.wrapping_add(1);
    }
}

/// A thread-safe handle to [`AppState`].
pub type SharedState = Arc<Mutex<AppState>>;

/// Optional, local-only examples offered when history is empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StarterPack {
    Developer,
    Writing,
}

/// A high-level user action emitted by the GUI, drained and handled by the app
/// wiring (which owns the store, clipboard, and paste backends).
#[derive(Clone, PartialEq, Eq)]
pub enum UiAction {
    /// Paste the given clip back into the previously focused app.
    Paste(ClipId),
    /// Paste an explicitly edited local composition draft.
    PasteText { text: String, sensitive: bool },
    /// Pin or unpin a clip.
    SetPinned(ClipId, bool),
    /// Exempt or unexempt a clip from capacity cleanup until this process exits.
    SetSessionProtected(ClipId, bool),
    /// Create a text-only derivative while preserving the canonical clip.
    CreatePlainTextClone(ClipId),
    /// Delete a single clip.
    Delete(ClipId),
    /// Restore one recently deleted in-memory clip.
    RestoreClip(Box<Clip>),
    /// Clear history while preserving pinned clips and cleanup exceptions.
    ClearHistory,
    /// Toggle capture pause.
    TogglePause,
    /// Explicitly keep the current clipboard after a recent privacy skip.
    RecoverSkipped,
    /// Install a small, explicit set of local example clips.
    InstallStarterPack(StarterPack),
    /// Apply one bounded first-run default profile.
    ApplyDefaultProfile(DefaultProfile),
    /// Enable or disable native launch-at-login registration.
    SetLaunchAtLogin(bool),
    /// Persist non-sensitive native popup preferences.
    SetUiPreferences {
        preferences: UiPreferences,
        reduced_motion_changed: bool,
    },
    /// Dismiss a de-duplicated health alert.
    DismissHealthAlert,
    /// Dismiss a de-duplicated size-budget alert.
    DismissSizeBudgetAlert,
    /// Dismiss the current command result.
    DismissNotice,
    /// Permanently dismiss the first-run hotkey coachmark.
    DismissHotkeyCoachmark,
    /// Close the popup; the runtime hides or exits based on resident availability.
    Hide,
    /// Exit the resident application.
    Quit,
}

impl fmt::Debug for UiAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Paste(id) => formatter.debug_tuple("Paste").field(id).finish(),
            Self::PasteText { text, sensitive } => formatter
                .debug_struct("PasteText")
                .field("text", &format_args!("[redacted; {} bytes]", text.len()))
                .field("sensitive", sensitive)
                .finish(),
            Self::SetPinned(id, pinned) => formatter
                .debug_tuple("SetPinned")
                .field(id)
                .field(pinned)
                .finish(),
            Self::SetSessionProtected(id, protected) => formatter
                .debug_tuple("SetSessionProtected")
                .field(id)
                .field(protected)
                .finish(),
            Self::CreatePlainTextClone(id) => formatter
                .debug_tuple("CreatePlainTextClone")
                .field(id)
                .finish(),
            Self::Delete(id) => formatter.debug_tuple("Delete").field(id).finish(),
            Self::RestoreClip(clip) => formatter
                .debug_struct("RestoreClip")
                .field("id", &clip.id)
                .field("kind", &clip.meta.kind)
                .field("bytes", &clip.meta.byte_size)
                .finish(),
            Self::ClearHistory => formatter.write_str("ClearHistory"),
            Self::TogglePause => formatter.write_str("TogglePause"),
            Self::RecoverSkipped => formatter.write_str("RecoverSkipped"),
            Self::InstallStarterPack(pack) => formatter
                .debug_tuple("InstallStarterPack")
                .field(pack)
                .finish(),
            Self::ApplyDefaultProfile(profile) => formatter
                .debug_tuple("ApplyDefaultProfile")
                .field(profile)
                .finish(),
            Self::SetLaunchAtLogin(enabled) => formatter
                .debug_tuple("SetLaunchAtLogin")
                .field(enabled)
                .finish(),
            Self::SetUiPreferences {
                preferences,
                reduced_motion_changed,
            } => formatter
                .debug_struct("SetUiPreferences")
                .field("preferences", preferences)
                .field("reduced_motion_changed", reduced_motion_changed)
                .finish(),
            Self::DismissHealthAlert => formatter.write_str("DismissHealthAlert"),
            Self::DismissSizeBudgetAlert => formatter.write_str("DismissSizeBudgetAlert"),
            Self::DismissNotice => formatter.write_str("DismissNotice"),
            Self::DismissHotkeyCoachmark => formatter.write_str("DismissHotkeyCoachmark"),
            Self::Hide => formatter.write_str("Hide"),
            Self::Quit => formatter.write_str("Quit"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_starts_capture_without_a_notice() {
        let state = AppState::with_clips(Vec::new());

        assert_eq!(state.capture_health, CaptureHealth::Starting);
        assert_eq!(
            state.security_posture.level,
            vbuff_types::SecurityPostureLevel::Partial
        );
        assert!(state.notice.is_none());
        assert!(!state.paused);
    }

    #[test]
    fn health_changes_are_deduplicated() {
        let mut state = AppState::default();

        assert!(state.set_capture_health(CaptureHealth::Watching));
        assert!(!state.set_capture_health(CaptureHealth::Watching));
        assert_eq!(state.capture_health.label(), "Capture active");
    }

    #[test]
    fn command_notice_can_be_replaced_and_cleared() {
        let mut state = AppState::default();
        state.set_notice(NoticeLevel::Warning, "Copy-only mode");
        assert_eq!(state.notice.as_ref().unwrap().level, NoticeLevel::Warning);

        state.clear_notice();
        assert!(state.notice.is_none());
    }

    #[test]
    fn skipped_recovery_offer_expires_and_is_single_use() {
        let mut state = AppState::default();
        let now = Instant::now();
        state.offer_skipped_recovery(now, Duration::from_secs(30));
        assert!(state.skipped_recovery_available(now + Duration::from_secs(29)));
        assert!(!state.skipped_recovery_available(now + Duration::from_secs(31)));

        state.offer_skipped_recovery(now, Duration::from_secs(30));
        assert!(state.take_skipped_recovery(now));
        assert!(!state.take_skipped_recovery(now));
    }
}
