//! Structured, explainable recall policies layered over immutable clips.

/// Unified query grammar (AST + facet registry + parser).
///
/// Exposed as its own namespace rather than flattened into `recall`: it
/// deliberately duplicates concepts that [`parse_natural_query`] still owns,
/// and keeping the two namespaces apart makes the migration (T1 in
/// `docs/solid-dry-review-2026-07-26.md`) a mechanical swap instead of a
/// rename war.
pub mod grammar;

mod graph;
mod layout;
pub use layout::{layout_contains, layout_variants};
mod memory;
mod query;
mod search;
mod source;

pub use graph::{ClipRelation, ClipRelationshipGraph, RelatedClip};
pub use memory::{
    ClipTags, PasteAffinity, PinnedAliases, QueryHistory, QueryPinSet, SearchMacroRegistry,
    SearchScope, SearchScopeLock,
};
pub use query::{NaturalQuery, QueryParseError, parse_natural_query};
pub use search::{
    MatchExplanation, MissSuggestion, RecallSearchContext, RecallSearchResult, SearchMiss,
    complete_query, search_recall,
};
pub use source::{FindSourceAction, find_source_action};
