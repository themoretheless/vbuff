use std::collections::{BTreeMap, BTreeSet};

use super::*;

#[test]
fn approval_requires_distinct_reviewer_and_resets_on_revision() {
    assert!(SnippetApprovalWorkflow::new("support.reply", [0; 32]).is_err());
    let mut workflow = SnippetApprovalWorkflow::new("support.reply", [1; 32]).unwrap();
    assert!(workflow.approve(TeamRole::Editor, [0; 32]).is_err());
    assert!(workflow.approve(TeamRole::Editor, [1; 32]).is_err());
    workflow.approve(TeamRole::Editor, [2; 32]).unwrap();
    workflow.publish(TeamRole::Owner).unwrap();
    workflow.revise(TeamRole::Editor, [3; 32]).unwrap();
    assert_eq!(workflow.state, SnippetPublicationState::Draft);
    assert_eq!(workflow.reviewer_hash, None);
    assert!(!format!("{workflow:?}").contains("3, 3, 3"));
}

#[test]
fn optional_receipts_store_only_bounded_fingerprints() {
    let mut disabled = ReadReceiptLedger::new(false);
    assert!(
        !disabled
            .record("collection", "item", &[7; 32], 100)
            .unwrap()
    );
    assert!(disabled.is_empty());

    let mut enabled = ReadReceiptLedger::new(true);
    assert!(enabled.record("collection", "item", &[7; 32], 100).unwrap());
    assert_eq!(enabled.len(), 1);
    let json = serde_json::to_string(&enabled).unwrap();
    assert!(!json.contains("collection"));
    assert!(!json.contains("item"));
    assert!(!format!("{enabled:?}").contains("read_at"));
    assert!(enabled.record("collection", "item", &[7; 32], -1).is_err());
}

#[test]
fn expiry_and_revocation_are_fail_closed() {
    let lease = SharedClipLease {
        item_id: "incident-note".into(),
        expires_at_ms: 200,
    };
    assert!(lease.is_active(199));
    assert!(!lease.is_active(200));
    assert!(!lease.is_active(-1));

    let mut share = ExternalShareGrant {
        share_id: "share-1".into(),
        item_hash: [4; 32],
        expires_at_ms: 500,
        revoked_at_ms: None,
    };
    assert!(share.is_active(100));
    share.revoke(150).unwrap();
    assert!(!share.is_active(150));
    assert!(!format!("{share:?}").contains("4, 4, 4"));
    assert!(
        ExternalShareGrant {
            revoked_at_ms: Some(-1),
            ..share
        }
        .validate()
        .is_err()
    );
    assert!(
        ExternalShareGrant {
            share_id: "share-2".into(),
            item_hash: [0; 32],
            expires_at_ms: 500,
            revoked_at_ms: None,
        }
        .validate()
        .is_err()
    );
}

#[test]
fn import_and_plugin_scopes_reject_unapproved_inputs() {
    let variables =
        SharedVariableCatalog::new(BTreeMap::from([("support.phone".into(), "555".into())]))
            .unwrap();
    let allowed_actions = BTreeSet::from(["copy_only".into()]);
    let validation = validate_team_import(
        &[
            TeamSnippetImport {
                snippet_id: "safe".into(),
                template: "Call {support.phone}".into(),
                variables: BTreeSet::from(["support.phone".into()]),
                action_ids: BTreeSet::from(["copy_only".into()]),
            },
            TeamSnippetImport {
                snippet_id: "unsafe".into(),
                template: "Run".into(),
                variables: BTreeSet::new(),
                action_ids: BTreeSet::from(["process.spawn".into()]),
            },
        ],
        &variables,
        &allowed_actions,
    );
    assert_eq!(validation.accepted_ids, vec!["safe"]);
    assert_eq!(validation.rejected["unsafe"], "unsafe_action");

    let approval = ScopedTeamPluginApproval {
        team_id: "support-team".into(),
        plugin_id: "dev.vbuff.clean".into(),
        manifest_hash: [9; 32],
        collection_ids: BTreeSet::from(["support".into()]),
        capability_ids: BTreeSet::from(["transform".into()]),
    };
    assert!(approval.allows("support-team", &[9; 32], "support", "transform"));
    assert!(!approval.allows("support-team", &[8; 32], "support", "transform"));
    assert!(!approval.allows("other-team", &[9; 32], "support", "transform"));
    assert!(!approval.allows("support-team", &[9; 32], "personal", "transform"));
    assert!(
        ScopedTeamPluginApproval {
            manifest_hash: [0; 32],
            ..approval
        }
        .validate()
        .is_err()
    );

    let duplicates = validate_team_import(
        &[
            TeamSnippetImport {
                snippet_id: "duplicate".into(),
                template: "First".into(),
                variables: BTreeSet::new(),
                action_ids: BTreeSet::new(),
            },
            TeamSnippetImport {
                snippet_id: "duplicate".into(),
                template: "Second".into(),
                variables: BTreeSet::new(),
                action_ids: BTreeSet::new(),
            },
        ],
        &variables,
        &allowed_actions,
    );
    assert!(duplicates.accepted_ids.is_empty());
    assert_eq!(duplicates.rejected["duplicate"], "duplicate_snippet_id");
}

#[test]
fn catalog_comments_broadcasts_and_audits_redact_payloads() {
    let catalog = SharedVariableCatalog::new(BTreeMap::from([(
        "product.url".into(),
        "https://example.invalid/private".into(),
    )]))
    .unwrap();
    assert!(!format!("{catalog:?}").contains("example.invalid"));

    let comment = ConflictComment {
        comment_id: "comment-1".into(),
        snippet_id: "snippet-1".into(),
        author_hash: [5; 32],
        body: "private discussion".into(),
        created_at_ms: 10,
    };
    comment.validate(TeamRole::Commenter).unwrap();
    assert!(!format!("{comment:?}").contains("private discussion"));

    let broadcast = EmergencyBroadcast {
        broadcast_id: "incident-1".into(),
        revision: 1,
        priority: BroadcastPriority::Emergency,
        message: "private response script".into(),
        expires_at_ms: 100,
    };
    broadcast.validate(TeamRole::Owner, 10).unwrap();
    assert!(!format!("{broadcast:?}").contains("private response"));

    let audit = TeamConfigAuditSnapshot {
        member_hash: [1; 32],
        policy_hash: [2; 32],
        policy_version: 4,
        capture_healthy: true,
        denied_source_count: 3,
        unavailable_capability_count: 1,
    };
    audit.validate().unwrap();
    let json = serde_json::to_string(&audit).unwrap();
    assert!(!json.contains("clip"));
    assert!(!json.contains("payload"));
    assert!(!format!("{audit:?}").contains("1, 1, 1"));
    assert!(
        TeamConfigAuditSnapshot {
            policy_hash: [0; 32],
            ..audit
        }
        .validate()
        .is_err()
    );
}

#[test]
fn changelog_is_contiguous_and_policy_simulation_is_synthetic_only() {
    let mut changelog = CollectionChangelog::default();
    changelog
        .append(CollectionChange {
            sequence: 1,
            actor_hash: [1; 32],
            kind: CollectionChangeKind::Policy,
            subject_hash: [2; 32],
            before_hash: None,
            after_hash: [3; 32],
            changed_at_ms: 10,
        })
        .unwrap();
    assert!(
        changelog
            .append(CollectionChange {
                sequence: 3,
                actor_hash: [1; 32],
                kind: CollectionChangeKind::Metadata,
                subject_hash: [2; 32],
                before_hash: None,
                after_hash: [4; 32],
                changed_at_ms: 11,
            })
            .is_err()
    );

    let policy = TeamDefaultDenylist {
        source_app_ids: BTreeSet::from(["password-manager".into()]),
        detector_ids: BTreeSet::from(["private_key".into()]),
    };
    let mut case = SyntheticPolicyCase {
        synthetic: false,
        source_app_id: "password-manager".into(),
        detector_ids: BTreeSet::new(),
    };
    assert!(simulate_team_policy(&policy, &case).is_err());
    case.synthetic = true;
    assert!(simulate_team_policy(&policy, &case).unwrap().denied);
    case.detector_ids.insert("invalid detector".into());
    assert!(simulate_team_policy(&policy, &case).is_err());
}
