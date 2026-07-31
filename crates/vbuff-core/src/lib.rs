//! Pure, OS-agnostic clipboard-history logic.
//!
//! This crate contains no I/O, no GUI, and no platform code. It is the
//! unit-testable heart of vbuff:
//!
//! * [`hash`] - canonical content hashing for deduplication.
//! * [`classify`] - heuristic content-kind detection.
#![forbid(unsafe_code)]

pub mod bloom;
pub mod capture;
pub mod classify;
pub mod clock;
pub mod compose;
pub mod delivery;
pub mod facets;
pub mod feedback;
pub mod fingerprint;
pub mod format_fidelity;
pub mod hash;
pub mod history_tier;
pub mod intelligence;
pub mod observability;
pub mod onboarding;
pub mod privacy;
pub mod recall;
pub mod reliability;
pub mod secret;
pub mod slo;
pub mod trust;
pub mod workflow;

pub use classify::detect_kind;
pub use hash::{content_hash, content_hash_from_flavors};
