use serde::{Deserialize, Serialize};

use super::{
    MutationMode, OperationContractError, OperationResult, RPC_SCHEMA_VERSION, validate_id,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupCommandPlan {
    pub operation_id: String,
    pub encrypted: bool,
    pub include_manifest: bool,
    pub verify_after_write: bool,
    pub mode: MutationMode,
}

impl BackupCommandPlan {
    pub fn validate(&self) -> OperationResult<()> {
        validate_id(&self.operation_id)?;
        if !self.encrypted || !self.include_manifest || !self.verify_after_write {
            return Err(OperationContractError::Invalid(
                "backup_guarantees_incomplete",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineHealthSnapshot {
    pub schema: u16,
    pub capture_state: String,
    pub database_bytes: u64,
    pub stored_items: u64,
    pub sync_queue_items: u64,
    pub degraded_capabilities: u32,
    pub checked_at_ms: i64,
}

impl MachineHealthSnapshot {
    pub fn validate(&self) -> OperationResult<()> {
        if self.schema != RPC_SCHEMA_VERSION {
            return Err(OperationContractError::Invalid("unsupported_schema"));
        }
        validate_id(&self.capture_state)?;
        if self.checked_at_ms < 0 {
            return Err(OperationContractError::Invalid("health_timestamp_invalid"));
        }
        Ok(())
    }

    pub fn canonical_json(&self) -> OperationResult<Vec<u8>> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|_| OperationContractError::Invalid("health_serialization_failed"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SanitizedFixtureManifest {
    pub schema: u16,
    pub fixture_id: String,
    pub item_count: usize,
    pub content_removed: bool,
    pub source_metadata_reviewed: bool,
    pub fixture_hash: [u8; 32],
}

impl SanitizedFixtureManifest {
    pub fn validate(&self) -> OperationResult<()> {
        if self.schema != RPC_SCHEMA_VERSION {
            return Err(OperationContractError::Invalid("unsupported_schema"));
        }
        validate_id(&self.fixture_id)?;
        if self.item_count == 0 || self.item_count > 1_024 {
            return Err(OperationContractError::Invalid(
                "fixture_item_count_invalid",
            ));
        }
        if self.fixture_hash == [0; 32] || !self.content_removed || !self.source_metadata_reviewed {
            return Err(OperationContractError::Invalid("fixture_not_sanitized"));
        }
        Ok(())
    }
}
