use std::fmt;

use thiserror::Error;
use vbuff_types::ClipId;

const MAX_INTEGRATION_ID_BYTES: usize = 128;
const MAX_REASON_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessDecision {
    AllowOnce,
    Deny,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum AccessRequestError {
    #[error("clip access request is invalid")]
    Invalid,
    #[error("clip access request was already decided")]
    AlreadyDecided,
}

#[derive(Clone)]
pub struct ClipAccessRequest {
    clip_id: ClipId,
    integration_id: String,
    reason: String,
    decided: bool,
}

impl fmt::Debug for ClipAccessRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClipAccessRequest")
            .field("clip_id", &"[redacted]")
            .field("integration_id", &"[redacted]")
            .field("reason_bytes", &self.reason.len())
            .field("decided", &self.decided)
            .finish()
    }
}

impl ClipAccessRequest {
    pub fn new(
        clip_id: ClipId,
        integration_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> Result<Self, AccessRequestError> {
        let integration_id = integration_id.into();
        let reason = reason.into();
        if !valid_identifier(&integration_id) || !valid_reason(&reason) {
            return Err(AccessRequestError::Invalid);
        }
        Ok(Self {
            clip_id,
            integration_id,
            reason,
            decided: false,
        })
    }

    pub fn integration_id(&self) -> &str {
        &self.integration_id
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub fn decide(
        &mut self,
        decision: AccessDecision,
        timestamp_ms: u64,
    ) -> Result<AccessAuditEntry, AccessRequestError> {
        if self.decided {
            return Err(AccessRequestError::AlreadyDecided);
        }
        self.decided = true;
        Ok(AccessAuditEntry {
            clip_id_hash: *blake3::hash(self.clip_id.to_string_repr().as_bytes()).as_bytes(),
            integration_id: self.integration_id.clone(),
            reason: self.reason.clone(),
            decision,
            timestamp_ms,
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct AccessAuditEntry {
    clip_id_hash: [u8; 32],
    integration_id: String,
    reason: String,
    decision: AccessDecision,
    timestamp_ms: u64,
}

impl fmt::Debug for AccessAuditEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AccessAuditEntry")
            .field("clip_id_hash", &"[redacted]")
            .field("integration_id", &"[redacted]")
            .field("reason_bytes", &self.reason.len())
            .field("decision", &self.decision)
            .field("timestamp_ms", &self.timestamp_ms)
            .finish()
    }
}

impl AccessAuditEntry {
    pub fn integration_id(&self) -> &str {
        &self.integration_id
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub const fn decision(&self) -> AccessDecision {
        self.decision
    }

    pub const fn timestamp_ms(&self) -> u64 {
        self.timestamp_ms
    }

    pub const fn clip_id_hash(&self) -> [u8; 32] {
        self.clip_id_hash
    }
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_INTEGRATION_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_reason(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_REASON_BYTES
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_requires_a_human_reason_and_produces_one_audit_decision() {
        assert!(ClipAccessRequest::new(ClipId::new(), "plugin.test", "  ").is_err());
        let mut request = ClipAccessRequest::new(
            ClipId::new(),
            "plugin.test",
            "Insert the selected snippet into the editor",
        )
        .unwrap();
        let audit = request.decide(AccessDecision::AllowOnce, 123).unwrap();
        assert_eq!(
            audit.reason(),
            "Insert the selected snippet into the editor"
        );
        assert_eq!(audit.decision(), AccessDecision::AllowOnce);
        assert_eq!(
            request.decide(AccessDecision::Deny, 124),
            Err(AccessRequestError::AlreadyDecided)
        );
        assert!(!format!("{request:?}").contains("Insert the selected"));
        assert!(!format!("{audit:?}").contains("plugin.test"));
    }
}
