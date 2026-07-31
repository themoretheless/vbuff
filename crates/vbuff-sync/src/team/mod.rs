//! Content-minimizing team collection governance contracts.
//!
//! These types do not provide a transport or activate shared collections.

mod approval;
mod audit;
mod import;
mod policy;
mod privacy;
mod sharing;

pub use approval::{SnippetApprovalWorkflow, SnippetPublicationState, TeamRole};
pub use audit::{CollectionChange, CollectionChangeKind, CollectionChangelog};
pub use import::{
    ScopedTeamPluginApproval, TeamImportValidation, TeamSnippetImport, validate_team_import,
};
pub use policy::{SyntheticPolicyCase, SyntheticPolicyDecision, simulate_team_policy};
pub use privacy::{
    ReadReceiptLedger, SharedVariableCatalog, TeamConfigAuditSnapshot, TeamDefaultDenylist,
};
pub use sharing::{
    BroadcastPriority, CollectionForkPlan, ConflictComment, EmergencyBroadcast, ExternalShareGrant,
    SharedClipLease,
};

use crate::{Result, SyncError};

const MAX_ID_BYTES: usize = 128;

pub(super) fn validate_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > MAX_ID_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return invalid("invalid team identifier");
    }
    Ok(())
}

pub(super) fn validate_hash(value: &[u8; 32], message: &str) -> Result<()> {
    if value == &[0; 32] {
        return invalid(message);
    }
    Ok(())
}

pub(super) fn invalid<T>(message: &str) -> Result<T> {
    Err(SyncError::Invalid(message.into()))
}

#[cfg(test)]
mod tests;
