use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use super::{MutationMode, OperationContractError, OperationResult, validate_id};
use crate::EventEnvelope;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadlessOperationKind {
    Import,
    Export,
    Backup,
    Health,
    FixtureExport,
}

impl HeadlessOperationKind {
    pub const fn requires_preview(self) -> bool {
        !matches!(self, Self::Health)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadlessOperationPlan {
    pub operation_id: String,
    pub kind: HeadlessOperationKind,
    pub launch_gui: bool,
    pub launch_tray: bool,
    pub mutating: bool,
    pub mode: MutationMode,
}

impl HeadlessOperationPlan {
    pub fn validate(&self) -> OperationResult<()> {
        validate_id(&self.operation_id)?;
        if self.launch_gui || self.launch_tray {
            return Err(OperationContractError::Invalid(
                "headless_operation_started_ui",
            ));
        }
        if self.mutating != self.kind.requires_preview() {
            return Err(OperationContractError::Invalid(
                "headless_mutation_class_mismatch",
            ));
        }
        if self.kind.requires_preview() && self.mode != MutationMode::DryRun {
            return Err(OperationContractError::Invalid(
                "mutating_headless_operation_requires_preview",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventReplayCursor {
    pub stream_id: String,
    pub next_sequence: u64,
}

impl EventReplayCursor {
    pub fn validate(&self) -> OperationResult<()> {
        validate_id(&self.stream_id)?;
        if self.next_sequence == 0 {
            return Err(OperationContractError::Invalid("invalid_replay_sequence"));
        }
        Ok(())
    }

    pub fn select<'a>(
        &self,
        retained_first_sequence: u64,
        events: &'a [EventEnvelope],
        limit: usize,
    ) -> OperationResult<Vec<&'a EventEnvelope>> {
        self.validate()?;
        if retained_first_sequence == 0 || limit == 0 {
            return Err(OperationContractError::Invalid("invalid_replay_window"));
        }
        if self.next_sequence < retained_first_sequence {
            return Err(OperationContractError::CursorExpired);
        }
        if events
            .iter()
            .any(|event| event.sequence < retained_first_sequence)
            || events
                .windows(2)
                .any(|pair| pair[0].sequence >= pair[1].sequence)
        {
            return Err(OperationContractError::Invalid(
                "replay_events_not_strictly_ordered",
            ));
        }
        Ok(events
            .iter()
            .filter(|event| event.sequence >= self.next_sequence)
            .take(limit.min(1_024))
            .collect())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopbackWebhookEndpoint {
    pub bind_address: IpAddr,
    pub port: u16,
    pub token_scope: String,
}

impl LoopbackWebhookEndpoint {
    pub fn validate(&self) -> OperationResult<()> {
        if !self.bind_address.is_loopback() || self.port == 0 {
            return Err(OperationContractError::Invalid(
                "webhook_must_bind_to_loopback",
            ));
        }
        validate_id(&self.token_scope)?;
        if self.token_scope != "webhook.ingress" {
            return Err(OperationContractError::Invalid(
                "webhook_token_scope_invalid",
            ));
        }
        Ok(())
    }
}
