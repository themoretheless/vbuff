use std::fmt;

use serde::{Deserialize, Serialize};
use vbuff_types::validation::{all_zero, is_valid_identifier, is_valid_label};

use super::IntegrationContractError;

mod remote;
mod share;
mod snippets;

pub use remote::{RemotePasteLease, RemotePasteRequest, RemoteReplayWindow};
pub use share::{ShareDraft, ShareDraftState};
pub use snippets::{
    SnippetBridgeCursor, SnippetMirrorAction, SnippetMirrorOperation, SnippetMirrorRecord,
    SnippetSyncManifest, SnippetSyncedState, VimRegisterAction, VimRegisterRequest,
    plan_snippet_mirror,
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
                return is_valid_identifier(device_id, 128)
                    .then_some(())
                    .ok_or(IntegrationContractError::InvalidField);
            }
        };
        is_valid_label(value, 64)
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
        if all_zero(&self.request_id)
            || self.issued_at_ms > now_ms
            || self.expires_at_ms <= self.issued_at_ms
            || self.expires_at_ms - self.issued_at_ms > 10 * 60 * 1_000
            || !is_valid_identifier(&self.target_device_id, 128)
        {
            return Err(IntegrationContractError::InvalidRecipient);
        }
        if now_ms >= self.expires_at_ms {
            return Err(IntegrationContractError::Expired);
        }
        if !is_valid_identifier(&self.clip_id, 128) {
            return Err(IntegrationContractError::InvalidField);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

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
        // Target matches the base, so the source edit is a safe upsert.
        let base = SnippetSyncManifest {
            entries: BTreeMap::from([(
                "deploy".to_string(),
                SnippetSyncedState::Present {
                    content_hash: [2; 32],
                },
            )]),
        };
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
            Some(&base),
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

    fn snippet_record(key: &str, hash_byte: u8, revision: u64) -> SnippetMirrorRecord {
        assert!(hash_byte != 0, "zero content hash is invalid");
        SnippetMirrorRecord {
            key: key.into(),
            content_hash: [hash_byte; 32],
            revision,
        }
    }

    fn snippet_manifest(entries: &[(&str, SnippetSyncedState)]) -> SnippetSyncManifest {
        SnippetSyncManifest {
            entries: entries
                .iter()
                .map(|(key, state)| ((*key).to_string(), state.clone()))
                .collect(),
        }
    }

    #[test]
    fn bug9a_diverged_edits_conflict_even_when_source_revision_is_higher() {
        let source = [snippet_record("deploy", 1, 5)];
        let target = [snippet_record("deploy", 2, 1)];
        // Both sides moved away from the base: the higher source revision must
        // not win, because the counters are independent, not causal.
        let base = snippet_manifest(&[(
            "deploy",
            SnippetSyncedState::Present {
                content_hash: [9; 32],
            },
        )]);
        let plan = plan_snippet_mirror(&source, &target, Some(&base)).unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].action, SnippetMirrorAction::Conflict);
        assert_eq!(plan[0].source_revision, 5);
        assert_eq!(plan[0].target_revision, 1);
        // Without a base there is no proof of an unchanged target either.
        let plan = plan_snippet_mirror(&source, &target, None).unwrap();
        assert_eq!(plan[0].action, SnippetMirrorAction::Conflict);
    }

    #[test]
    fn bug9b_target_only_record_conflicts_instead_of_silent_delete() {
        let target = [snippet_record("local-only", 3, 7)];
        // No base at all.
        let plan = plan_snippet_mirror(&[], &target, None).unwrap();
        assert_eq!(plan[0].action, SnippetMirrorAction::Conflict);
        // Trusted base that never saw the key.
        let plan = plan_snippet_mirror(&[], &target, Some(&SnippetSyncManifest::default())).unwrap();
        assert_eq!(plan[0].action, SnippetMirrorAction::Conflict);
        // Tombstoned in the base but present on target: still no safe delete.
        let base = snippet_manifest(&[("local-only", SnippetSyncedState::Deleted)]);
        let plan = plan_snippet_mirror(&[], &target, Some(&base)).unwrap();
        assert_eq!(plan[0].action, SnippetMirrorAction::Conflict);
    }

    #[test]
    fn delete_requires_tombstone_proof_and_unchanged_target() {
        let target = [snippet_record("old", 4, 3)];
        // Target holds exactly what the base recorded: the one auto-delete.
        let base = snippet_manifest(&[(
            "old",
            SnippetSyncedState::Present {
                content_hash: [4; 32],
            },
        )]);
        let plan = plan_snippet_mirror(&[], &target, Some(&base)).unwrap();
        assert_eq!(plan[0].action, SnippetMirrorAction::DeleteTarget);
        // Target moved away from the base: conflict, not deletion.
        let base = snippet_manifest(&[(
            "old",
            SnippetSyncedState::Present {
                content_hash: [9; 32],
            },
        )]);
        let plan = plan_snippet_mirror(&[], &target, Some(&base)).unwrap();
        assert_eq!(plan[0].action, SnippetMirrorAction::Conflict);
    }

    #[test]
    fn upsert_requires_target_unchanged_relative_to_base() {
        let source = [snippet_record("deploy", 1, 5)];
        let target = [snippet_record("deploy", 2, 1)];
        // target == base: safe upsert of the source edit.
        let base = snippet_manifest(&[(
            "deploy",
            SnippetSyncedState::Present {
                content_hash: [2; 32],
            },
        )]);
        let plan = plan_snippet_mirror(&source, &target, Some(&base)).unwrap();
        assert_eq!(plan[0].action, SnippetMirrorAction::UpsertTarget);
        // source == base: target edited, reverse sync is deliberately not
        // planned, so this is a conflict.
        let base = snippet_manifest(&[(
            "deploy",
            SnippetSyncedState::Present {
                content_hash: [1; 32],
            },
        )]);
        let plan = plan_snippet_mirror(&source, &target, Some(&base)).unwrap();
        assert_eq!(plan[0].action, SnippetMirrorAction::Conflict);
        // Source-only brand-new key: nothing to overwrite on target.
        let plan = plan_snippet_mirror(&source, &[], None).unwrap();
        assert_eq!(plan[0].action, SnippetMirrorAction::UpsertTarget);
        // Source-only against a base-Present key: edit-vs-delete conflict.
        let plan = plan_snippet_mirror(&source, &[], Some(&base)).unwrap();
        assert_eq!(plan[0].action, SnippetMirrorAction::Conflict);
    }

    #[test]
    fn re_add_after_tombstone_is_upsert() {
        let base = snippet_manifest(&[("deploy", SnippetSyncedState::Deleted)]);
        let source = [snippet_record("deploy", 1, 1)];
        let plan = plan_snippet_mirror(&source, &[], Some(&base)).unwrap();
        assert_eq!(plan[0].action, SnippetMirrorAction::UpsertTarget);
    }

    #[test]
    fn untrusted_manifest_degrades_everything_to_conflicts() {
        let manifest = snippet_manifest(&[(
            "deploy",
            SnippetSyncedState::Present {
                content_hash: [2; 32],
            },
        )]);
        let mut cursor = SnippetBridgeCursor {
            adapter: "espanso".into(),
            source_revision: 0,
            target_revision: 0,
            last_manifest_hash: manifest.compute_hash(),
            manifest,
        };
        assert!(cursor.trusted_manifest().is_some());
        // Tampered hash: fail-closed to "no base", so every changed or
        // target-only key conflicts instead of being upserted or deleted.
        cursor.last_manifest_hash = [0xAB; 32];
        assert!(cursor.trusted_manifest().is_none());
        let source = [snippet_record("deploy", 1, 5)];
        let target = [snippet_record("deploy", 2, 1)];
        let plan = plan_snippet_mirror(&source, &target, cursor.trusted_manifest()).unwrap();
        assert_eq!(plan[0].action, SnippetMirrorAction::Conflict);
        let plan = plan_snippet_mirror(&[], &target, cursor.trusted_manifest()).unwrap();
        assert_eq!(plan[0].action, SnippetMirrorAction::Conflict);
    }

    #[test]
    fn legacy_cursor_without_manifest_deserializes_and_is_distrusted() {
        let legacy = serde_json::json!({
            "adapter": "espanso",
            "source_revision": 1,
            "target_revision": 2,
            "last_manifest_hash": vec![7_u8; 32],
        });
        let cursor: SnippetBridgeCursor = serde_json::from_value(legacy).unwrap();
        assert!(cursor.manifest.entries.is_empty());
        assert!(cursor.trusted_manifest().is_none());
    }

    #[test]
    fn manifest_hash_is_deterministic_and_validate_is_bounded() {
        let first = snippet_manifest(&[
            (
                "alpha",
                SnippetSyncedState::Present {
                    content_hash: [1; 32],
                },
            ),
            ("beta", SnippetSyncedState::Deleted),
        ]);
        // Insertion order must not matter: the BTreeMap canonicalizes it.
        let second = snippet_manifest(&[
            ("beta", SnippetSyncedState::Deleted),
            (
                "alpha",
                SnippetSyncedState::Present {
                    content_hash: [1; 32],
                },
            ),
        ]);
        assert_eq!(first.compute_hash(), first.compute_hash());
        assert_eq!(first.compute_hash(), second.compute_hash());
        let different = snippet_manifest(&[(
            "alpha",
            SnippetSyncedState::Present {
                content_hash: [2; 32],
            },
        )]);
        assert_ne!(first.compute_hash(), different.compute_hash());
        assert!(first.validate().is_ok());
        // Same key rules as snippet_map: control characters are rejected.
        let bad_key = snippet_manifest(&[("bad\nkey", SnippetSyncedState::Deleted)]);
        assert!(bad_key.validate().is_err());
        // Zero content hash is invalid in Present, same as in records.
        let zero_hash = snippet_manifest(&[(
            "alpha",
            SnippetSyncedState::Present {
                content_hash: [0; 32],
            },
        )]);
        assert!(zero_hash.validate().is_err());
        let oversized = SnippetSyncManifest {
            entries: (0..10_001)
                .map(|index| (format!("key-{index}"), SnippetSyncedState::Deleted))
                .collect(),
        };
        assert!(oversized.validate().is_err());
    }

    #[test]
    fn applied_contract_updates_manifest_and_caller_rehashes_cursor() {
        let base = snippet_manifest(&[
            (
                "deploy",
                SnippetSyncedState::Present {
                    content_hash: [2; 32],
                },
            ),
            (
                "old",
                SnippetSyncedState::Present {
                    content_hash: [4; 32],
                },
            ),
            (
                "conflict",
                SnippetSyncedState::Present {
                    content_hash: [8; 32],
                },
            ),
        ]);
        let source = [snippet_record("deploy", 1, 5), snippet_record("conflict", 6, 9)];
        let target = [
            snippet_record("deploy", 2, 1),
            snippet_record("old", 4, 3),
            snippet_record("conflict", 7, 2),
        ];
        let plan = plan_snippet_mirror(&source, &target, Some(&base)).unwrap();
        let next = base.applied(&plan, &source, &target).unwrap();
        // UpsertTarget records the source hash.
        assert_eq!(
            next.entries.get("deploy"),
            Some(&SnippetSyncedState::Present {
                content_hash: [1; 32]
            })
        );
        // DeleteTarget records a tombstone.
        assert_eq!(next.entries.get("old"), Some(&SnippetSyncedState::Deleted));
        // Conflict leaves the entry unchanged.
        assert_eq!(
            next.entries.get("conflict"),
            Some(&SnippetSyncedState::Present {
                content_hash: [8; 32]
            })
        );
        assert!(next.validate().is_ok());
        // The caller recomputes last_manifest_hash; the helper never does.
        let mut cursor = SnippetBridgeCursor {
            adapter: "espanso".into(),
            source_revision: 5,
            target_revision: 3,
            last_manifest_hash: base.compute_hash(),
            manifest: base.clone(),
        };
        assert!(cursor.trusted_manifest().is_some());
        cursor.manifest = next;
        assert!(cursor.trusted_manifest().is_none());
        cursor.last_manifest_hash = cursor.manifest.compute_hash();
        assert!(cursor.trusted_manifest().is_some());
        // Operations referencing unknown keys are rejected fail-closed.
        let stray = SnippetMirrorOperation {
            key_hash: [0xFF; 32],
            action: SnippetMirrorAction::UpsertTarget,
            source_revision: 1,
            target_revision: 0,
        };
        assert!(base.applied(&[stray], &source, &target).is_err());
    }

    #[test]
    fn new_manifest_types_redact_content_in_debug() {
        let state = SnippetSyncedState::Present {
            content_hash: [1; 32],
        };
        let debug = format!("{state:?}");
        assert!(debug.contains("[redacted]"));
        assert!(!debug.contains("1, 1"));
        let manifest = snippet_manifest(&[("deploy", state)]);
        assert!(!format!("{manifest:?}").contains("deploy"));
        let cursor = SnippetBridgeCursor {
            adapter: "espanso".into(),
            source_revision: 0,
            target_revision: 0,
            last_manifest_hash: manifest.compute_hash(),
            manifest,
        };
        assert!(!format!("{cursor:?}").contains("deploy"));
    }
}
