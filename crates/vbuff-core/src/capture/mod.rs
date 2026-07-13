//! Pure capture-gate, coalescing, integrity, and accounting logic.

mod coalesce;
mod integrity;
mod ledger;
mod policy;
mod scheduler;

pub use coalesce::{
    CoalesceDecision, FlavorGrowthCoalescer, PrimaryIntent, PrimaryIntentGate, TransformRelation,
    relate_text,
};
pub use integrity::{
    IntegrityFailure, PrunedFlavor, annotate_integrity, prune_redundant_flavors, verify_integrity,
};
pub use ledger::{
    CaptureCounters, CaptureLossLedger, GenerationObservation, GenerationTracker,
    SelfTestObservation, SelfTestState, SelfWriteLedger, SkippedCapture, SkippedCaptureRing,
};
pub use policy::{
    CaptureAction, CaptureDecision, CaptureInput, CaptureOutcome, CapturePolicy, CaptureRule,
    DropClass, DropReason, SelectionSource, SourcePredicate,
};
pub use scheduler::{AdaptivePollScheduler, BudgetObservation, PollObservation, SubsystemBudget};
