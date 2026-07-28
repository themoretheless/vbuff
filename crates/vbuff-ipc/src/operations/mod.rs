//! Versioned contracts for future CLI and local daemon operations.

mod completion;
mod diagnostics;
mod rate_limit;
mod rpc;
mod runtime;

pub use completion::{CompletionCandidate, CompletionKind, ShellCompletionCatalog};
pub use diagnostics::{BackupCommandPlan, MachineHealthSnapshot, SanitizedFixtureManifest};
pub use rate_limit::{RateLimitKind, RateLimitPolicy, TokenRateLimiter};
pub use rpc::{MutationMode, MutationPreview, MutationRequest, RpcEnvelope};
pub use runtime::{
    EventReplayCursor, HeadlessOperationKind, HeadlessOperationPlan, LoopbackWebhookEndpoint,
};

use thiserror::Error;

pub const RPC_SCHEMA_VERSION: u16 = 1;
const MAX_ID_BYTES: usize = 128;
pub(super) const MAX_COMPLETION_RESULTS: usize = 50;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum OperationContractError {
    #[error("invalid operation contract: {0}")]
    Invalid(&'static str),
    #[error("event replay cursor is outside the retained window")]
    CursorExpired,
    #[error("operation rate limit exceeded")]
    RateLimited,
}

pub type OperationResult<T> = std::result::Result<T, OperationContractError>;

pub(super) fn validate_id(value: &str) -> OperationResult<()> {
    if value.is_empty()
        || value.len() > MAX_ID_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(OperationContractError::Invalid("invalid_identifier"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
