//! Local-only contextual onboarding and recap eligibility.

use std::collections::BTreeSet;

use crate::clock::Clock;

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
}
