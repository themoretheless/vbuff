//! Bounded, side-effect-free models for temporary clipboard workflows.

mod boards;
mod compare;
mod everyday;
mod links;
mod selection;
mod snippets;
mod timeline;

pub use boards::{
    ActionCandidate, BoardContext, BoardError, BoardItem, BoardMatcher, BoardRouter,
    CaptureCollector, Checklist, CollectorJoiner, ConsumeQueue, NamedSlots, PinBoard,
    SessionBasket, rank_actions,
};
pub use compare::{
    CompareError, DiffChunk, DiffKind, DiffMode, TextTransform, TransformHistory, TransformOverlay,
    TransformRecord, compare_text,
};
pub use everyday::{
    BehaviorAction, CleanLinkMemory, CopyBurst, DomainRuleSuggestion, PinReviewCandidate,
    RuleSuggestion, RuleSuggestionEngine, SessionProtection, SizeBudgetDecision, SizeBudgetError,
    expiry_label, group_copy_bursts, plain_text_clone, recent_source_apps, stale_pin_candidates,
};
pub use links::{CleanLinkError, clean_link};
pub use selection::{ExampleFilter, RangeSelection, SelectionAggregate, filter_from_example};
pub use snippets::{
    ComputedField, FieldDefinition, FieldKind, FieldValue, FormEvaluation, SnippetError,
    SnippetForm, ValuePredicate, VisibilityRule,
};
pub use timeline::{
    SessionClip, TimelineBucket, TimelineError, TimelineGranularity, WorkSession,
    group_work_sessions, timeline_buckets,
};
