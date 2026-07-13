use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use vbuff_types::{CaptureGeneration, Flavor, FlavorRealization};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoalesceDecision {
    Wait,
    Commit,
    Restarted,
}

/// Closes a burst after two observations with the same complete flavor set.
#[derive(Debug, Default)]
pub struct FlavorGrowthCoalescer {
    generation: Option<CaptureGeneration>,
    signature: BTreeSet<(String, u64, FlavorRealization)>,
    stable_observations: u8,
}

impl FlavorGrowthCoalescer {
    pub fn observe(
        &mut self,
        generation: Option<CaptureGeneration>,
        flavors: &[Flavor],
    ) -> CoalesceDecision {
        let signature = flavors
            .iter()
            .map(|flavor| {
                (
                    flavor.mime.to_ascii_lowercase(),
                    flavor.body.byte_size(),
                    flavor.realization,
                )
            })
            .collect::<BTreeSet<_>>();
        let has_pending = flavors
            .iter()
            .any(|flavor| flavor.realization == FlavorRealization::Deferred);

        if self.generation.is_some() && generation != self.generation {
            self.generation = generation;
            self.signature = signature;
            self.stable_observations = 0;
            return CoalesceDecision::Restarted;
        }
        self.generation = generation;

        if signature != self.signature {
            self.signature = signature;
            self.stable_observations = 0;
            return CoalesceDecision::Wait;
        }
        if has_pending {
            self.stable_observations = 0;
            return CoalesceDecision::Wait;
        }

        self.stable_observations = self.stable_observations.saturating_add(1);
        if self.stable_observations >= 2 {
            CoalesceDecision::Commit
        } else {
            CoalesceDecision::Wait
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrimaryIntent {
    pub modifier_seen: bool,
    pub middle_click_seen: bool,
}

/// Requires PRIMARY to remain stable and have an observable user intent.
#[derive(Debug)]
pub struct PrimaryIntentGate {
    settle: Duration,
    first_seen: Option<Instant>,
    last_hash: Option<[u8; 32]>,
}

impl PrimaryIntentGate {
    pub fn new(settle: Duration) -> Self {
        Self {
            settle,
            first_seen: None,
            last_hash: None,
        }
    }

    pub fn observe(&mut self, hash: [u8; 32], intent: PrimaryIntent, now: Instant) -> bool {
        if self.last_hash != Some(hash) {
            self.last_hash = Some(hash);
            self.first_seen = Some(now);
            return false;
        }
        let stable = self
            .first_seen
            .is_some_and(|start| now.saturating_duration_since(start) >= self.settle);
        stable && (intent.modifier_seen || intent.middle_click_seen)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransformRelation {
    Exact,
    LineEndings,
    Whitespace,
    CaseOnly,
}

pub fn relate_text(previous: &str, next: &str) -> Option<TransformRelation> {
    if previous == next {
        return Some(TransformRelation::Exact);
    }
    if previous.replace("\r\n", "\n") == next.replace("\r\n", "\n") {
        return Some(TransformRelation::LineEndings);
    }
    let fold = |value: &str| value.split_whitespace().collect::<Vec<_>>().join(" ");
    if fold(previous) == fold(next) {
        return Some(TransformRelation::Whitespace);
    }
    if previous.to_lowercase() == next.to_lowercase() {
        return Some(TransformRelation::CaseOnly);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalescer_waits_for_growth_to_stop_twice() {
        let mut coalescer = FlavorGrowthCoalescer::default();
        let text = Flavor::inline("text/plain", b"hello".to_vec());
        let html = Flavor::inline("text/html", b"<p>hello</p>".to_vec());

        assert_eq!(
            coalescer.observe(None, std::slice::from_ref(&text)),
            CoalesceDecision::Wait
        );
        assert_eq!(
            coalescer.observe(None, &[text.clone(), html.clone()]),
            CoalesceDecision::Wait
        );
        assert_eq!(
            coalescer.observe(None, &[text.clone(), html.clone()]),
            CoalesceDecision::Wait
        );
        assert_eq!(
            coalescer.observe(None, &[text, html]),
            CoalesceDecision::Commit
        );
    }

    #[test]
    fn primary_requires_stability_and_intent() {
        let start = Instant::now();
        let mut gate = PrimaryIntentGate::new(Duration::from_millis(80));
        let hash = [7; 32];
        assert!(!gate.observe(hash, PrimaryIntent::default(), start));
        assert!(!gate.observe(
            hash,
            PrimaryIntent::default(),
            start + Duration::from_millis(90)
        ));
        assert!(gate.observe(
            hash,
            PrimaryIntent {
                modifier_seen: true,
                middle_click_seen: false,
            },
            start + Duration::from_millis(100)
        ));
    }

    #[test]
    fn recognizes_common_transform_churn() {
        assert_eq!(
            relate_text("a\r\nb", "a\nb"),
            Some(TransformRelation::LineEndings)
        );
        assert_eq!(
            relate_text("a   b", "a b"),
            Some(TransformRelation::Whitespace)
        );
        assert_eq!(
            relate_text("Hello", "HELLO"),
            Some(TransformRelation::CaseOnly)
        );
        assert_eq!(relate_text("one", "two"), None);
    }
}
