//! Privacy and trust decisions that stay independent from GUI and native APIs.

mod consent;
mod posture;
mod secrets;

pub use consent::EphemeralCountdown;
pub use posture::{PrivacyPostureInput, PrivacyScore, PrivacyScoreFactor, PrivacyScoreLevel};
pub use secrets::{
    DetectorUpdateError, SecretHandling, SecretMask, SensitivityReason, SignedDetectorUpdate,
    handling_for_secret, sensitivity_reason_for_secret, sensitivity_watermark,
};
