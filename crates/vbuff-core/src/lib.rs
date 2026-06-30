//! Pure, OS-agnostic clipboard-history logic.
//!
//! This crate contains no I/O, no GUI, and no platform code. It is the
//! unit-testable heart of vbuff:
//!
//! * [`hash`] - canonical content hashing for deduplication.
//! * [`classify`] - heuristic content-kind detection.
//! * [`filter`] - case-insensitive search + ranking (pinned first, then recency).
//! * [`eviction`] - retention policy (cap N, never evict pinned).

pub mod classify;
pub mod eviction;
pub mod filter;
pub mod hash;

pub use classify::detect_kind;
pub use eviction::{evict, EvictionPolicy};
pub use filter::{search, SearchResult};
pub use hash::{content_hash, content_hash_from_flavors};
