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

/// Background recall results are tied to both the query and history revision.
#[derive(Clone)]
pub struct HistorySearchResults {
    pub query: String,
    pub scope: crate::experience::HistoryScope,
    pub history_revision: u64,
    pub clips: Arc<[Clip]>,
    pub total: usize,
    pub failed: bool,
}

/// The live state the GUI renders. Owned behind a [`SharedState`] lock so the
/// background capture thread can push new clips while the GUI reads them.
#[derive(Default)]
pub struct AppState {
    pub tags: Arc<vbuff_types::TagSnapshot>,
    pub history_search: Option<HistorySearchResults>,
    /// The current clip list, already ordered (pinned first, then recency).
    ///
    /// Replace it only through [`AppState::set_clips`]. The GUI's projection
    /// cache treats [`AppState::revision`] as this list's identity, so a
    /// replacement that skips the bump would leave a stale list on screen.
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
    ///
    /// This is the GUI projection cache's clip-list identity, so the bump is a
    /// contract rather than an optimization: see the test at the bottom of this
    /// file, and `ProjectionCache` for what the cache does with it. Bumping it
    /// for something other than a clip change is harmless (it only costs a
    /// recompute); *not* bumping it for a clip change is not.
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
        self.history_search = None;
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

/// Clipboard-derived text carried by an action or command.
///
/// This is the single redaction point for user content in the command
/// vocabulary: its `Debug` prints only a byte count, so every type that carries
/// it can `#[derive(Debug)]` and stay leak-free without a hand-written impl to
/// forget. `Display` is deliberately not implemented — reaching the bytes must
/// be an explicit [`ClipText::into_string`] call, never an accidental `{}` in a
/// log line.
#[derive(Clone, PartialEq, Eq)]
pub struct ClipText(String);

impl ClipText {
    /// Wrap text that came from (or is headed to) the clipboard.
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// Take ownership of the content for a deliberate, non-logging use.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Debug for ClipText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "[redacted; {} bytes]", self.0.len())
    }
}

/// A recently deleted clip travelling back to the store.
///
/// Same contract as [`ClipText`]: one redaction point, so the carriers derive
/// `Debug`. The summary stays at id/kind/size rather than leaning on
/// [`vbuff_types::Body`]'s redacted `Debug`, so a restore never widens what a
/// log line says about a clip.
#[derive(Clone, PartialEq, Eq)]
pub struct RestoredClip(Box<Clip>);

impl RestoredClip {
    /// Wrap a clip that is being restored.
    pub fn new(clip: Box<Clip>) -> Self {
        Self(clip)
    }

    /// Take ownership of the clip for a deliberate, non-logging use.
    pub fn into_clip(self) -> Clip {
        *self.0
    }
}

impl fmt::Debug for RestoredClip {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RestoredClip")
            .field("id", &self.0.id)
            .field("kind", &self.0.meta.kind)
            .field("bytes", &self.0.meta.byte_size)
            .finish()
    }
}

/// Defines the enum below verbatim and, from the same variant list, the
/// `VARIANT_COUNT` the redaction guard checks itself against.
///
/// The indirection buys exactly one thing: a new variant cannot be added
/// without the guard noticing. `variant_name` stops compiling (its match is
/// exhaustive) *and* the sample count no longer matches, so the new variant has
/// to be given a sample carrying clipboard content before the suite is green
/// again. A hand-written constant would just as silently stay correct-looking.
macro_rules! redaction_guarded_enum {
    (
        $(#[$enum_meta:meta])*
        pub enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident
                    $(( $($tuple_ty:ty),* $(,)? ))?
                    $({ $($field:ident : $field_ty:ty),* $(,)? })?
            ),* $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        pub enum $name {
            $(
                $(#[$variant_meta])*
                $variant
                    $(( $($tuple_ty),* ))?
                    $({ $($field : $field_ty),* })?
            ),*
        }

        impl $name {
            /// Variant count, taken from the definition above rather than
            /// maintained by hand.
            #[cfg(test)]
            const VARIANT_COUNT: usize = [$(stringify!($variant)),*].len();
        }
    };
}

redaction_guarded_enum! {
    /// A high-level user action emitted by the GUI, drained and handled by the
    /// app wiring (which owns the store, clipboard, and paste backends).
    ///
    /// `Debug` is derived on purpose. Any variant that carries clipboard
    /// content must carry it as [`ClipText`] or [`RestoredClip`], whose own
    /// `Debug` impls redact; a bare `String`/`Vec<u8>` payload here would print
    /// verbatim into the logs. There is deliberately no hand-written `Debug` on
    /// this type to forget an arm in, and the guard test at the bottom of this
    /// file refuses to compile — then refuses to pass — when a variant is added
    /// without a sample.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum UiAction {
        EditTags(vbuff_types::TagCommand),
        /// Paste the given clip back into the previously focused app.
        Paste(ClipId),
        /// Paste an explicitly edited local composition draft.
        PasteText { text: ClipText, sensitive: bool },
        /// Pin or unpin a clip.
        SetPinned(ClipId, bool),
        SetTtl(ClipId, Option<u64>),
        /// Exempt or unexempt a clip from capacity cleanup until this process exits.
        SetSessionProtected(ClipId, bool),
        /// Create a text-only derivative while preserving the canonical clip.
        CreatePlainTextClone(ClipId),
        /// Delete a single clip.
        Delete(ClipId),
        /// Restore one recently deleted in-memory clip.
        RestoreClip(RestoredClip),
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
        /// Close the popup; the runtime always hides it and never exits here —
        /// exiting happens only through an explicit [`UiAction::Quit`].
        Hide,
        /// Exit the resident application.
        Quit,
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

    /// The GUI projection cache keys on `revision` to decide whether the clip
    /// list it projected is still the current one. Every replacement has to
    /// move it, including a replacement with an equal-length or empty list.
    #[test]
    fn replacing_the_clip_list_always_bumps_the_revision() {
        let mut state = AppState::with_clips(vec![clip_with_content()]);
        let start = state.revision;

        state.set_clips(vec![clip_with_content()]);
        assert_eq!(state.revision, start.wrapping_add(1));

        // Same length, different contents: nothing about the list's shape
        // signals the change, so the counter is the only signal there is.
        state.set_clips(vec![clip_with_content()]);
        assert_eq!(state.revision, start.wrapping_add(2));

        state.set_clips(Vec::new());
        assert_eq!(state.revision, start.wrapping_add(3));
        assert!(state.clips.is_empty());
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

    /// Clipboard content: must never appear in a rendered `Debug`.
    const CLIP_CONTENT: &str = "topsecret-clipboard-content";
    /// The restore summary stays at id/kind/size, so capture metadata that
    /// names the user's app never reaches a log line either.
    const SOURCE_APP: &str = "com.example.PrivateJournal";

    fn clip_with_content() -> Clip {
        let flavors = vec![vbuff_types::Flavor::inline(
            "text/plain;charset=utf-8",
            CLIP_CONTENT.as_bytes().to_vec(),
        )];
        Clip {
            id: ClipId::new(),
            content_hash: [0; 32],
            meta: vbuff_types::ClipMeta::now(
                vbuff_types::ContentKind::Text,
                CLIP_CONTENT.len() as u64,
                Some(SOURCE_APP.to_owned()),
            ),
            flavors,
            pinned: false,
            favorite: false,
        }
    }

    /// One sample per [`UiAction`] variant, each carrying content where the
    /// variant can carry any.
    fn every_action() -> Vec<UiAction> {
        vec![
            UiAction::EditTags(vbuff_types::TagCommand::Save {
                id: None,
                name: CLIP_CONTENT.into(),
                color: [0; 3],
            }),
            UiAction::Paste(ClipId::new()),
            UiAction::PasteText {
                text: ClipText::new(CLIP_CONTENT),
                sensitive: true,
            },
            UiAction::SetPinned(ClipId::new(), true),
            UiAction::SetTtl(ClipId::new(), Some(3600)),
            UiAction::SetSessionProtected(ClipId::new(), true),
            UiAction::CreatePlainTextClone(ClipId::new()),
            UiAction::Delete(ClipId::new()),
            UiAction::RestoreClip(RestoredClip::new(Box::new(clip_with_content()))),
            UiAction::ClearHistory,
            UiAction::TogglePause,
            UiAction::RecoverSkipped,
            UiAction::InstallStarterPack(StarterPack::Developer),
            UiAction::ApplyDefaultProfile(DefaultProfile::PrivacyMax),
            UiAction::SetLaunchAtLogin(true),
            UiAction::SetUiPreferences {
                preferences: UiPreferences::default(),
                reduced_motion_changed: true,
            },
            UiAction::DismissHealthAlert,
            UiAction::DismissSizeBudgetAlert,
            UiAction::DismissNotice,
            UiAction::DismissHotkeyCoachmark,
            UiAction::Hide,
            UiAction::Quit,
        ]
    }

    /// Exhaustive on purpose: adding a variant stops this file compiling, which
    /// is the forcing function that keeps the redaction guard complete.
    fn variant_name(action: &UiAction) -> &'static str {
        match action {
            UiAction::EditTags(_) => "EditTags",
            UiAction::Paste(_) => "Paste",
            UiAction::PasteText { .. } => "PasteText",
            UiAction::SetPinned(..) => "SetPinned",
            UiAction::SetTtl(..) => "SetTtl",
            UiAction::SetSessionProtected(..) => "SetSessionProtected",
            UiAction::CreatePlainTextClone(_) => "CreatePlainTextClone",
            UiAction::Delete(_) => "Delete",
            UiAction::RestoreClip(_) => "RestoreClip",
            UiAction::ClearHistory => "ClearHistory",
            UiAction::TogglePause => "TogglePause",
            UiAction::RecoverSkipped => "RecoverSkipped",
            UiAction::InstallStarterPack(_) => "InstallStarterPack",
            UiAction::ApplyDefaultProfile(_) => "ApplyDefaultProfile",
            UiAction::SetLaunchAtLogin(_) => "SetLaunchAtLogin",
            UiAction::SetUiPreferences { .. } => "SetUiPreferences",
            UiAction::DismissHealthAlert => "DismissHealthAlert",
            UiAction::DismissSizeBudgetAlert => "DismissSizeBudgetAlert",
            UiAction::DismissNotice => "DismissNotice",
            UiAction::DismissHotkeyCoachmark => "DismissHotkeyCoachmark",
            UiAction::Hide => "Hide",
            UiAction::Quit => "Quit",
        }
    }

    #[test]
    fn debug_redacts_clip_content_for_every_variant() {
        let mut covered = HashSet::new();
        for action in every_action() {
            let rendered = format!("{action:?}");
            assert!(
                !rendered.contains(CLIP_CONTENT),
                "{} leaked clip content: {rendered}",
                variant_name(&action)
            );
            assert!(
                !rendered.contains(SOURCE_APP),
                "{} leaked capture metadata: {rendered}",
                variant_name(&action)
            );
            covered.insert(variant_name(&action));
        }

        assert_eq!(
            covered.len(),
            UiAction::VARIANT_COUNT,
            "every UiAction variant needs a sample in every_action()"
        );
    }

    #[test]
    fn clip_text_debug_reports_only_a_byte_count() {
        let text = ClipText::new(CLIP_CONTENT);

        assert_eq!(
            format!("{text:?}"),
            format!("[redacted; {} bytes]", CLIP_CONTENT.len())
        );
        assert_eq!(text.into_string(), CLIP_CONTENT);
    }

    #[test]
    fn restored_clip_debug_reports_only_identity_and_size() {
        let clip = clip_with_content();
        let id = clip.id;
        let restored = RestoredClip::new(Box::new(clip));
        let rendered = format!("{restored:?}");

        assert!(rendered.contains(&format!("{id:?}")), "{rendered}");
        assert!(
            rendered.contains(&format!("bytes: {}", CLIP_CONTENT.len())),
            "{rendered}"
        );
        assert!(!rendered.contains(CLIP_CONTENT), "{rendered}");
        assert_eq!(restored.into_clip().id, id);
    }

    #[test]
    fn paste_text_debug_keeps_the_sensitive_flag_visible() {
        let action = UiAction::PasteText {
            text: ClipText::new("private draft"),
            sensitive: true,
        };
        let rendered = format!("{action:?}");

        assert!(!rendered.contains("private draft"), "{rendered}");
        assert!(rendered.contains("sensitive: true"), "{rendered}");
    }
}
