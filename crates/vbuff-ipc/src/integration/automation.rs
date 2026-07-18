use std::fmt;

use serde::{Deserialize, Serialize};

use super::IntegrationContractError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationSurface {
    Shortcuts,
    Tasker,
    VimRegister,
    Tmux,
    MobileShareSheet,
    MobileQuickAction,
    OsShareTarget,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum AutomationCommand {
    GetLatest,
    AddClip { tag: Option<String> },
    PasteByTag { tag: String },
    SendToDevice { device_id: String },
}

impl AutomationCommand {
    pub fn validate(&self) -> Result<(), IntegrationContractError> {
        let value = match self {
            Self::GetLatest => return Ok(()),
            Self::AddClip { tag: None } => return Ok(()),
            Self::AddClip { tag: Some(tag) } | Self::PasteByTag { tag } => tag,
            Self::SendToDevice { device_id } => {
                return valid_identifier(device_id, 128)
                    .then_some(())
                    .ok_or(IntegrationContractError::InvalidField);
            }
        };
        valid_label(value, 64)
            .then_some(())
            .ok_or(IntegrationContractError::InvalidField)
    }
}

impl fmt::Debug for AutomationCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GetLatest => formatter.write_str("GetLatest"),
            Self::AddClip { tag } => formatter
                .debug_struct("AddClip")
                .field("tag_bytes", &tag.as_ref().map(String::len))
                .finish(),
            Self::PasteByTag { tag } => formatter
                .debug_struct("PasteByTag")
                .field("tag_bytes", &tag.len())
                .finish(),
            Self::SendToDevice { device_id } => formatter
                .debug_struct("SendToDevice")
                .field("device_id_bytes", &device_id.len())
                .finish(),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemotePasteRequest {
    pub forwarded_socket: String,
    pub session_nonce: String,
    pub clip_id: String,
}

impl fmt::Debug for RemotePasteRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemotePasteRequest")
            .field("forwarded_socket_bytes", &self.forwarded_socket.len())
            .field("session_nonce", &"[redacted]")
            .field("clip_id", &"[redacted]")
            .finish()
    }
}

impl RemotePasteRequest {
    pub fn validate(&self) -> Result<(), IntegrationContractError> {
        for value in [&self.forwarded_socket, &self.session_nonce, &self.clip_id] {
            if value.is_empty()
                || value.len() > 256
                || !value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/' | b':')
                })
            {
                return Err(IntegrationContractError::InvalidField);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareDraftState {
    Preview,
    Committed,
    Cancelled,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShareDraft {
    pub draft_id: String,
    pub destination_collection: Option<String>,
    pub tags: Vec<String>,
    pub pinned: bool,
    pub state: ShareDraftState,
}

impl fmt::Debug for ShareDraft {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ShareDraft")
            .field("draft_id", &"[redacted]")
            .field(
                "destination_collection_bytes",
                &self.destination_collection.as_ref().map(String::len),
            )
            .field("tag_count", &self.tags.len())
            .field("pinned", &self.pinned)
            .field("state", &self.state)
            .finish()
    }
}

impl ShareDraft {
    pub fn commit(&mut self) -> Result<(), IntegrationContractError> {
        if self.state != ShareDraftState::Preview
            || !valid_identifier(&self.draft_id, 128)
            || self
                .destination_collection
                .as_ref()
                .is_some_and(|collection| !valid_label(collection, 128))
            || self.tags.len() > 32
            || self.tags.iter().any(|tag| !valid_label(tag, 64))
        {
            return Err(IntegrationContractError::InvalidField);
        }
        self.state = ShareDraftState::Committed;
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnippetBridgeCursor {
    pub adapter: String,
    pub source_revision: u64,
    pub target_revision: u64,
    pub last_manifest_hash: [u8; 32],
}

impl fmt::Debug for SnippetBridgeCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SnippetBridgeCursor")
            .field("adapter_bytes", &self.adapter.len())
            .field("source_revision", &self.source_revision)
            .field("target_revision", &self.target_revision)
            .field("last_manifest_hash", &"[redacted]")
            .finish()
    }
}

impl SnippetBridgeCursor {
    pub fn accepts(&self, source_revision: u64, target_revision: u64) -> bool {
        source_revision >= self.source_revision
            && target_revision >= self.target_revision
            && (source_revision > self.source_revision || target_revision > self.target_revision)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetedSendRequest {
    pub request_id: [u8; 16],
    pub clip_id: String,
    pub target_device_id: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
}

impl fmt::Debug for TargetedSendRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TargetedSendRequest")
            .field("request_id", &"[redacted]")
            .field("clip_id", &"[redacted]")
            .field("target_device_id", &"[redacted]")
            .field("issued_at_ms", &self.issued_at_ms)
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

impl TargetedSendRequest {
    pub fn validate(&self, now_ms: u64) -> Result<(), IntegrationContractError> {
        if self.request_id.iter().all(|byte| *byte == 0)
            || self.issued_at_ms > now_ms
            || self.expires_at_ms <= self.issued_at_ms
            || self.expires_at_ms - self.issued_at_ms > 10 * 60 * 1_000
            || !valid_identifier(&self.target_device_id, 128)
        {
            return Err(IntegrationContractError::InvalidRecipient);
        }
        if now_ms >= self.expires_at_ms {
            return Err(IntegrationContractError::Expired);
        }
        if !valid_identifier(&self.clip_id, 128) {
            return Err(IntegrationContractError::InvalidField);
        }
        Ok(())
    }
}

fn valid_identifier(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_label(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= maximum_bytes && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_ingress_cannot_commit_without_preview_state() {
        let mut draft = ShareDraft {
            draft_id: "draft-1".into(),
            destination_collection: Some("work".into()),
            tags: vec!["review".into()],
            pinned: false,
            state: ShareDraftState::Preview,
        };
        draft.commit().unwrap();
        assert_eq!(draft.state, ShareDraftState::Committed);
        assert!(draft.commit().is_err());

        let mut invalid_destination = ShareDraft {
            draft_id: "draft-2".into(),
            destination_collection: Some("bad\ncollection".into()),
            tags: Vec::new(),
            pinned: false,
            state: ShareDraftState::Preview,
        };
        assert!(invalid_destination.commit().is_err());
    }

    #[test]
    fn targeted_send_is_one_recipient_and_short_lived() {
        let request = TargetedSendRequest {
            request_id: [1; 16],
            clip_id: "01HCLIP".into(),
            target_device_id: "phone".into(),
            issued_at_ms: 100,
            expires_at_ms: 200,
        };
        assert!(request.validate(150).is_ok());
        assert_eq!(
            request.validate(200),
            Err(IntegrationContractError::Expired)
        );
        let multiple = TargetedSendRequest {
            target_device_id: "phone,laptop".into(),
            ..request.clone()
        };
        assert_eq!(
            multiple.validate(150),
            Err(IntegrationContractError::InvalidRecipient)
        );
        let missing_nonce = TargetedSendRequest {
            request_id: [0; 16],
            target_device_id: "phone".into(),
            ..request
        };
        assert_eq!(
            missing_nonce.validate(150),
            Err(IntegrationContractError::InvalidRecipient)
        );
    }

    #[test]
    fn automation_commands_bound_every_user_identifier() {
        assert!(AutomationCommand::GetLatest.validate().is_ok());
        assert!(
            AutomationCommand::AddClip {
                tag: Some("review".into())
            }
            .validate()
            .is_ok()
        );
        assert!(
            AutomationCommand::SendToDevice {
                device_id: "phone,laptop".into()
            }
            .validate()
            .is_err()
        );
        assert!(
            AutomationCommand::PasteByTag {
                tag: "x".repeat(65)
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn remote_paste_contract_rejects_shell_metacharacters() {
        let request = RemotePasteRequest {
            forwarded_socket: "localhost:/run/user/1000/vbuff.sock".into(),
            session_nonce: "private-nonce-1".into(),
            clip_id: "clip-1".into(),
        };
        assert!(request.validate().is_ok());
        let debug = format!("{request:?}");
        assert!(!debug.contains("private-nonce"));
        assert!(!debug.contains("localhost"));
        assert!(
            RemotePasteRequest {
                forwarded_socket: "socket;rm".into(),
                session_nonce: "nonce".into(),
                clip_id: "clip".into(),
            }
            .validate()
            .is_err()
        );
    }
}
