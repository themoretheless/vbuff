//! Structured, explainable recall policies layered over immutable clips.

mod graph;
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
