//! Privacy and trust decisions that stay independent from GUI and native APIs.

mod consent;
mod posture;
mod rules;
mod secrets;

pub use consent::{
    EphemeralCountdown, ExternalAction, LocalOnlyWorkspacePolicy, SensitiveSourceChoice,
    SensitiveSourceConsent, SensitiveSourceDecision, UnlockTimeouts,
};
pub use posture::{PrivacyPostureInput, PrivacyScore, PrivacyScoreFactor, PrivacyScoreLevel};
pub use rules::{
    CaptureRule, CaptureRuleAction, CaptureRuleSimulator, PasteGuard, PasteGuardDecision,
    RuleMatchReason, SimulationError, SimulationInput, SimulationResult,
};
pub use secrets::{
    DetectorUpdateError, SecretHandling, SecretMask, SensitivityReason, SignedDetectorUpdate,
    handling_for_secret, sensitivity_reason_for_secret, sensitivity_watermark,
};
