//! Local-only contextual onboarding and recap eligibility.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::clock::Clock;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultProfile {
    #[default]
    Casual,
    Developer,
    PrivacyMax,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileDefaults {
    pub max_history: usize,
    pub secret_ttl_seconds: u64,
    pub capture_soft_limit_bytes: usize,
    pub capture_hard_limit_bytes: usize,
    pub auto_pause_idle_seconds: u64,
    pub auto_pause_remote: bool,
    pub detect_secrets: bool,
}

impl DefaultProfile {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Casual => "Casual",
            Self::Developer => "Developer",
            Self::PrivacyMax => "Privacy Max",
        }
    }

    pub const fn defaults(self) -> ProfileDefaults {
        const MIB: usize = 1024 * 1024;
        match self {
            Self::Casual => ProfileDefaults {
                max_history: 500,
                secret_ttl_seconds: 10 * 60,
                capture_soft_limit_bytes: 16 * MIB,
                capture_hard_limit_bytes: 128 * MIB,
                auto_pause_idle_seconds: 15 * 60,
                auto_pause_remote: true,
                detect_secrets: true,
            },
            Self::Developer => ProfileDefaults {
                max_history: 2_000,
                secret_ttl_seconds: 10 * 60,
                capture_soft_limit_bytes: 32 * MIB,
                capture_hard_limit_bytes: 256 * MIB,
                auto_pause_idle_seconds: 30 * 60,
                auto_pause_remote: true,
                detect_secrets: true,
            },
            Self::PrivacyMax => ProfileDefaults {
                max_history: 200,
                secret_ttl_seconds: 60,
                capture_soft_limit_bytes: 4 * MIB,
                capture_hard_limit_bytes: 32 * MIB,
                auto_pause_idle_seconds: 5 * 60,
                auto_pause_remote: true,
                detect_secrets: true,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BehaviorEvent {
    FirstImageCaptured,
    TenthClipCaptured,
    FirstSearchMiss,
    FirstSensitiveSkip,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NudgeKind {
    PreviewImage,
    PinUsefulClip,
    RefineSearch,
    ReviewPrivacyLedger,
}

#[derive(Clone, Debug, Default)]
pub struct NudgeEngine {
    shown: BTreeSet<NudgeKind>,
    dismissed: BTreeSet<NudgeKind>,
}

impl NudgeEngine {
    pub fn observe(&mut self, event: BehaviorEvent) -> Option<NudgeKind> {
        let nudge = match event {
            BehaviorEvent::FirstImageCaptured => NudgeKind::PreviewImage,
            BehaviorEvent::TenthClipCaptured => NudgeKind::PinUsefulClip,
            BehaviorEvent::FirstSearchMiss => NudgeKind::RefineSearch,
            BehaviorEvent::FirstSensitiveSkip => NudgeKind::ReviewPrivacyLedger,
        };
        if self.shown.contains(&nudge) || self.dismissed.contains(&nudge) {
            return None;
        }
        self.shown.insert(nudge);
        Some(nudge)
    }

    pub fn dismiss(&mut self, nudge: NudgeKind) {
        self.dismissed.insert(nudge);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalRecap {
    pub recalled: u64,
    pub captured: u64,
    pub pinned: u64,
    pub estimated_retypes_avoided: u64,
}

pub fn day_seven_recap(
    clock: &impl Clock,
    installed_at_ms: u64,
    recalled: u64,
    captured: u64,
    pinned: u64,
) -> Option<LocalRecap> {
    const SEVEN_DAYS_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
    (clock.now_ms().saturating_sub(installed_at_ms) >= SEVEN_DAYS_MS).then_some(LocalRecap {
        recalled,
        captured,
        pinned,
        estimated_retypes_avoided: recalled,
    })
}

#[cfg(test)]
mod tests {
    use crate::clock::ManualClock;

    use super::*;

    #[test]
    fn nudges_are_contextual_single_use_and_recap_stays_local() {
        let mut engine = NudgeEngine::default();
        assert_eq!(
            engine.observe(BehaviorEvent::FirstImageCaptured),
            Some(NudgeKind::PreviewImage)
        );
        assert_eq!(engine.observe(BehaviorEvent::FirstImageCaptured), None);

        let clock = ManualClock::new(7 * 24 * 60 * 60 * 1_000);
        let recap = day_seven_recap(&clock, 0, 42, 80, 6).unwrap();
        assert_eq!(recap.recalled, 42);
        assert_eq!(recap.estimated_retypes_avoided, 42);
    }

    #[test]
    fn first_run_profiles_are_concrete_and_privacy_max_is_stricter() {
        let casual = DefaultProfile::Casual.defaults();
        let developer = DefaultProfile::Developer.defaults();
        let private = DefaultProfile::PrivacyMax.defaults();

        assert!(developer.max_history > casual.max_history);
        assert!(private.max_history < casual.max_history);
        assert!(private.secret_ttl_seconds < casual.secret_ttl_seconds);
        assert!(private.auto_pause_idle_seconds < casual.auto_pause_idle_seconds);
        assert!(private.auto_pause_remote);
        assert!(developer.auto_pause_remote);
    }
}
