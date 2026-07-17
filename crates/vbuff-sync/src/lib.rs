//! OS- and transport-independent E2E sync protocol primitives.
#![forbid(unsafe_code)]

pub mod bootstrap;
pub mod burn;
pub mod capability;
pub mod clock;
pub mod collection_vault;
pub mod conflict;
pub mod crdt;
pub mod crypto;
pub mod ledger;
pub mod membership;
pub mod merkle;
pub mod policy;
pub mod provenance;
pub mod vault_export;
pub mod wire;

mod error;

pub use error::{Result, SyncError};
