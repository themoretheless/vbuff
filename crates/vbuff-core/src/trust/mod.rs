//! Privacy and trust decisions that stay independent from GUI and native APIs.

mod access;
mod consent;
mod posture;
mod rules;
mod secrets;

pub use access::{AccessAuditEntry, AccessDecision, AccessRequestError, ClipAccessRequest};
pub use consent::{
    EphemeralCountdown, ExternalAction, LocalOnlyWorkspacePolicy, SensitiveSourceChoice,
    SensitiveSourceConsent, SensitiveSourceDecision, UnlockTimeouts,
};
pub use posture::{
    CrashDiagnosticCounters, PrivacyPostureInput, PrivacyScore, PrivacyScoreFactor,
    PrivacyScoreLevel, RedactedPolicyExport, RuleAuditCounters, RuleCounterView,
    SealedCrashDiagnostics,
};
pub use rules::{
    CaptureRule, CaptureRuleAction, CaptureRuleSimulator, PasteGuard, PasteGuardDecision,
    RuleMatchReason, SimulationError, SimulationInput, SimulationResult,
};
pub use secrets::{
    DetectorUpdateError, SecretHandling, SecretMask, SensitivityReason, SignedDetectorUpdate,
    handling_for_secret, sensitivity_reason_for_secret, sensitivity_watermark,
};
