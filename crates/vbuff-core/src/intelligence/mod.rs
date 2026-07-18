//! Privacy-gated, local-first intelligence contracts and deterministic fallbacks.

mod actions;
mod budget;
mod gate;
mod grouping;

pub use actions::{
    ActiveTagger, CaptionBackend, CaptionError, ClipExplanation, IntentAction, PasteDestination,
    PasteGuardDecision, PasteGuardFingerprint, PiiDetectorBackend, PiiFinding, RulePiiDetector,
    SmartPastePlan, classify_intent, explain_text, plan_smart_paste,
};
pub use budget::{InferenceBudget, InferenceDecision, InferenceQueue, PowerState};
pub use gate::{AiGate, AiOperation, AiRefusal};
pub use grouping::{ThreadCandidate, near_duplicate, thread_candidates};
