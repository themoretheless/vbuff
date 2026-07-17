use vbuff_core::capture::{CaptureLossLedger, GenerationObservation, GenerationTracker};
use vbuff_types::CaptureGeneration;

#[test]
fn fifty_thousand_consecutive_native_generations_have_zero_accounted_loss() {
    let mut generations = GenerationTracker::default();
    let mut loss = CaptureLossLedger::default();

    for sequence in 1..=50_000 {
        let observation = generations.observe(CaptureGeneration { epoch: 7, sequence });
        assert!(matches!(
            observation,
            GenerationObservation::First | GenerationObservation::Consecutive
        ));
        loss.captured();
    }

    let counters = loss.snapshot();
    assert_eq!(counters.captured, 50_000);
    assert_eq!(counters.intentionally_skipped, 0);
    assert_eq!(counters.lost, 0);
    assert!(counters.by_reason.is_empty());
}
