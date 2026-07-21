use thiserror::Error;

/// Shared fail-closed error vocabulary for transport-independent integrations.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum IntegrationContractError {
    #[error("integration field is invalid")]
    InvalidField,
    #[error("integration request has expired")]
    Expired,
    #[error("integration request is not scoped to one recipient")]
    InvalidRecipient,
}
