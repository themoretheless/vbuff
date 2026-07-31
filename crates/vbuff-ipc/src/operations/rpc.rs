use serde::{Deserialize, Serialize};

use super::{OperationContractError, OperationResult, RPC_SCHEMA_VERSION, validate_id};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcEnvelope<T> {
    pub schema: u16,
    pub request_id: String,
    pub payload: T,
}

impl<T> std::fmt::Debug for RpcEnvelope<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RpcEnvelope")
            .field("schema", &self.schema)
            .field("request_id", &self.request_id)
            .field("payload", &"[redacted]")
            .finish()
    }
}

impl<T> RpcEnvelope<T> {
    pub fn validate_header(&self) -> OperationResult<()> {
        if self.schema != RPC_SCHEMA_VERSION {
            return Err(OperationContractError::Invalid("unsupported_schema"));
        }
        validate_id(&self.request_id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationMode {
    DryRun,
    Apply,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationRequest<T> {
    pub mode: MutationMode,
    pub operation: T,
}

impl<T> std::fmt::Debug for MutationRequest<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MutationRequest")
            .field("mode", &self.mode)
            .field("operation", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationPreview {
    pub operation_id: String,
    pub affected_items: usize,
    pub estimated_byte_delta: i64,
    pub warning_ids: Vec<String>,
}

impl MutationPreview {
    pub fn validate(&self) -> OperationResult<()> {
        validate_id(&self.operation_id)?;
        if self.warning_ids.len() > 32
            || self
                .warning_ids
                .iter()
                .any(|warning_id| validate_id(warning_id).is_err())
        {
            return Err(OperationContractError::Invalid("invalid_warning_ids"));
        }
        Ok(())
    }
}
