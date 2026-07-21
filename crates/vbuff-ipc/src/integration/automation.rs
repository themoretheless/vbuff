use std::fmt;

use serde::{Deserialize, Serialize};

use super::IntegrationContractError;

mod remote;
mod share;
mod snippets;

pub use remote::{RemotePasteLease, RemotePasteRequest, RemoteReplayWindow};
pub use share::{ShareDraft, ShareDraftState};
pub use snippets::{
    SnippetBridgeCursor, SnippetMirrorAction, SnippetMirrorOperation, SnippetMirrorRecord,
    VimRegisterAction, VimRegisterRequest, plan_snippet_mirror,
};

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
        let mut draft = ShareDraft::preview(
            "draft-1".into(),
            Some("work".into()),
            vec!["review".into()],
            false,
        )
        .unwrap();
        draft.commit().unwrap();
        assert_eq!(draft.state(), ShareDraftState::Committed);
        assert_eq!(draft.draft_id(), "draft-1");
        assert_eq!(draft.destination_collection(), Some("work"));
        assert_eq!(draft.tags(), &["review"]);
        assert!(!draft.pinned());
        assert!(draft.commit().is_err());

        let mut cancelled = ShareDraft::preview("draft-3".into(), None, Vec::new(), true).unwrap();
        cancelled.cancel().unwrap();
        assert_eq!(cancelled.state(), ShareDraftState::Cancelled);
        assert!(cancelled.commit().is_err());

        assert!(
            ShareDraft::preview(
                "draft-2".into(),
                Some("bad\ncollection".into()),
                Vec::new(),
                false,
            )
            .is_err()
        );
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
        assert!(
            RemotePasteRequest {
                forwarded_socket: "localhost:/run/../private/vbuff.sock".into(),
                session_nonce: "nonce".into(),
                clip_id: "clip".into(),
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn remote_paste_lease_is_short_lived_authenticated_and_one_shot() {
        let request = RemotePasteRequest {
            forwarded_socket: "localhost:/run/user/1000/vbuff.sock".into(),
            session_nonce: "nonce-1".into(),
            clip_id: "clip-1".into(),
        };
        let lease = RemotePasteLease::bind(&request, &[7; 32], 100, 1_000).unwrap();
        let mut window = RemoteReplayWindow::default();
        window
            .verify_and_consume(&lease, &request, &[7; 32], 500)
            .unwrap();
        assert!(
            window
                .verify_and_consume(&lease, &request, &[7; 32], 501)
                .is_err()
        );
        assert!(!format!("{lease:?}").contains("nonce-1"));
        assert_eq!(
            format!("{window:?}"),
            "RemoteReplayWindow { consumed_count: 1 }"
        );
        assert!(
            RemoteReplayWindow::default()
                .verify_and_consume(&lease, &request, &[8; 32], 500)
                .is_err()
        );
        assert!(RemotePasteLease::bind(&request, &[7; 32], 100, 60_001).is_err());
        assert!(RemotePasteLease::bind(&request, &[0; 32], 100, 1_000).is_err());
    }

    #[test]
    fn snippet_mirror_and_vim_register_are_bounded_and_content_free() {
        let plan = plan_snippet_mirror(
            &[SnippetMirrorRecord {
                key: "deploy".into(),
                content_hash: [1; 32],
                revision: 2,
            }],
            &[SnippetMirrorRecord {
                key: "deploy".into(),
                content_hash: [2; 32],
                revision: 1,
            }],
        )
        .unwrap();
        assert_eq!(plan[0].action, SnippetMirrorAction::UpsertTarget);
        assert!(!format!("{:?}", plan[0]).contains("deploy"));
        let source = SnippetMirrorRecord {
            key: "deploy".into(),
            content_hash: [1; 32],
            revision: 2,
        };
        assert!(!format!("{source:?}").contains("deploy"));
        assert!(
            VimRegisterRequest {
                namespace: "vbuff".into(),
                slot: 12,
                action: VimRegisterAction::ReadHistory,
            }
            .validate()
            .is_ok()
        );
        assert!(
            VimRegisterRequest {
                namespace: "system".into(),
                slot: 0,
                action: VimRegisterAction::AddYank,
            }
            .validate()
            .is_err()
        );
    }
}
