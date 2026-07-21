use std::collections::BTreeSet;

use vbuff_types::ContentKind;
use x25519_dalek::{PublicKey, StaticSecret};

use super::*;
use crate::clock::HybridLogicalClock;
use crate::conflict::{ConflictCandidate, ConflictReason};
use crate::crypto::open_sealed;

fn policy(trust: DeviceTrustLevel, mode: BandwidthMode) -> DeviceExperiencePolicy {
    DeviceExperiencePolicy {
        device_id: "phone".into(),
        trust,
        clipboard_write_allowed: false,
        retention_days: Some(7),
        mask_sensitive_history: true,
        bandwidth_mode: mode,
        nearby_verified: true,
        last_seen_ms: 100,
    }
}

fn item(id: &str, sensitive: bool) -> SyncItemSummary {
    SyncItemSummary {
        item_id: id.into(),
        collection: Some("work".into()),
        kind: ContentKind::Text,
        byte_size: 10,
        sensitive,
        sync_eligible: !sensitive,
        has_thumbnail: false,
        created_at_ms: 1_000,
    }
}

#[test]
fn trust_rehearsal_replay_and_write_policy_fail_closed() {
    let mut receive = policy(DeviceTrustLevel::ReceiveOnly, BandwidthMode::MetadataFirst);
    let items = vec![item("public", false), item("secret", true)];
    let rehearsal = rehearse_pairing(&receive, &items).unwrap();
    assert_eq!(rehearsal.transferable_items, 1);
    assert_eq!(rehearsal.local_only_items, 1);
    assert_eq!(rehearsal.metadata_first_items, 1);
    assert!(!receive.allows_live_clipboard_write());
    receive.clipboard_write_allowed = true;
    assert!(!receive.allows_live_clipboard_write());

    let replay = plan_selective_replay(
        &receive,
        &items,
        &ReplaySelection {
            collections: BTreeSet::from(["work".into()]),
            maximum_items: 10,
            ..ReplaySelection::default()
        },
    )
    .unwrap();
    assert_eq!(replay.item_ids, vec!["public"]);
    assert_eq!(replay.skipped_local_only, 1);
    assert!(!format!("{replay:?}").contains("public"));
}

#[test]
fn revocation_is_encrypted_and_conflicts_are_explainable() {
    let recipient_secret = StaticSecret::from([5; 32]);
    let recipient_public = PublicKey::from(&recipient_secret).to_bytes();
    let sealed = sealed_revocation_tombstones(
        "phone",
        &["clip-1".into(), "clip-2".into()],
        3,
        100,
        &recipient_public,
    )
    .unwrap();
    let opened = open_sealed(&recipient_secret, &sealed, b"vbuff-revoke-v1:3:100").unwrap();
    let tombstones: Vec<RevocationTombstone> = serde_json::from_slice(&opened).unwrap();
    assert_eq!(tombstones.len(), 2);
    assert!(
        !sealed
            .ciphertext
            .windows(6)
            .any(|window| window == b"clip-1")
    );

    let timeline = conflict_timeline(
        ConflictCandidate {
            value: [1; 32],
            clock: HybridLogicalClock::new("laptop", 10),
        },
        ConflictCandidate {
            value: [2; 32],
            clock: HybridLogicalClock::new("phone", 11),
        },
    )
    .unwrap();
    assert_eq!(timeline.len(), 2);
    assert_eq!(timeline[1].outcome, ConflictTimelineOutcome::Winner);
    assert_eq!(timeline[1].reason, ConflictReason::NewerPhysicalTime);
}

#[test]
fn outbox_bandwidth_retention_and_travel_state_are_inspectable() {
    let mut outbox = SyncOutbox::default();
    let event_id = [1; 16];
    outbox
        .enqueue(OutboxEntry {
            event_id,
            item_hash: [2; 32],
            target_device_hash: [3; 32],
            attempts: 0,
            status: OutboxStatus::Pending,
            next_retry_ms: None,
            last_error_code: None,
        })
        .unwrap();
    outbox.record_retry(event_id, 500, "timeout").unwrap();
    assert_eq!(
        outbox.entries().next().unwrap().status,
        OutboxStatus::WaitingRetry
    );
    outbox.mark_delivered(event_id).unwrap();
    assert_eq!(
        outbox.entries().next().unwrap().status,
        OutboxStatus::Delivered
    );
    assert!(outbox.record_retry(event_id, 600, "timeout").is_err());
    assert!(outbox.mark_delivered(event_id).is_err());

    let policy = policy(DeviceTrustLevel::FullTrust, BandwidthMode::OnDemand);
    let item = item("clip", false);
    assert_eq!(
        transfer_materialization(&policy, &item, false),
        TransferMaterialization::MetadataOnly
    );
    assert_eq!(
        effective_retention_deadline_ms(&policy, &item, 30).unwrap(),
        Some(1_000 + 7 * 24 * 60 * 60 * 1_000)
    );
    let travel = TravelMode {
        enabled: true,
        enabled_at_ms: 100,
        expires_at_ms: Some(200),
        retention_hours: 24,
    };
    travel.validate().unwrap();
    assert!(travel.active(150).unwrap());
    assert!(!travel.sync_allowed(150));
    assert!(!travel.sensitive_visible(150));
    assert!(travel.sync_allowed(200));
}

#[test]
fn qr_nearby_and_dry_run_expose_no_clip_payload() {
    let mut token = QrHandoffToken::issue([9; 32], 100, 1_000).unwrap();
    let payload = token.payload();
    let encoded = payload
        .split('/')
        .next_back()
        .unwrap()
        .split('?')
        .next()
        .unwrap();
    assert!(token.consume(encoded, 500));
    assert!(!token.consume(encoded, 501));
    assert!(!token.consume(encoded, 1_100));
    assert!(!payload.contains("private clip"));
    assert!(!format!("{token:?}").contains(encoded));

    let targets = nearby_send_targets(
        &[
            policy(DeviceTrustLevel::FullTrust, BandwidthMode::Full),
            DeviceExperiencePolicy {
                device_id: "untrusted".into(),
                trust: DeviceTrustLevel::Untrusted,
                nearby_verified: true,
                ..policy(DeviceTrustLevel::FullTrust, BandwidthMode::Full)
            },
        ],
        200,
    )
    .unwrap();
    assert_eq!(targets.len(), 1);
    assert!(!format!("{:?}", targets[0]).contains("phone"));
    let estimate = estimate_sync(
        &[policy(
            DeviceTrustLevel::FullTrust,
            BandwidthMode::MetadataFirst,
        )],
        &[item("public", false), item("secret", true)],
    )
    .unwrap();
    assert_eq!(estimate.item_count, 1);
    assert_eq!(estimate.sensitive_local_only, 1);
    assert_eq!(estimate.affected_devices, 1);
    assert_eq!(estimate.metadata_first_items, 1);
    let duplicate = policy(DeviceTrustLevel::ReceiveOnly, BandwidthMode::OnDemand);
    assert!(estimate_sync(&[duplicate.clone(), duplicate], &[]).is_err());
}

#[test]
fn shared_snippet_requires_distinct_review_and_current_base() {
    let author = [1; 32];
    let reviewer = [2; 32];
    let mut proposal = SharedSnippetProposal::new([3; 16], [4; 32], author, 7).unwrap();
    let allowed = BTreeSet::from([reviewer]);
    assert!(proposal.approve(reviewer, &allowed, 1).unwrap());
    assert!(proposal.commit(6).is_err());
    assert_eq!(proposal.commit(7).unwrap(), 8);
    assert_eq!(proposal.state(), SharedSnippetState::Committed);

    let mut impossible = SharedSnippetProposal::new([7; 16], [8; 32], author, 1).unwrap();
    assert!(
        impossible
            .approve(reviewer, &BTreeSet::from([author, reviewer]), 2)
            .is_err()
    );

    let mut self_approval = SharedSnippetProposal::new([5; 16], [6; 32], author, 1).unwrap();
    assert!(
        self_approval
            .approve(author, &BTreeSet::from([author]), 1)
            .is_err()
    );
}

#[test]
fn privacy_boundaries_reject_invalid_transfer_inputs() {
    let trusted = policy(DeviceTrustLevel::FullTrust, BandwidthMode::Full);
    let public = item("public", false);
    let private = item("private", true);
    assert_eq!(
        transfer_materialization(&trusted, &private, true),
        TransferMaterialization::Denied
    );
    assert_eq!(
        transfer_materialization(
            &policy(DeviceTrustLevel::Untrusted, BandwidthMode::Full),
            &public,
            true,
        ),
        TransferMaterialization::Denied
    );
    assert!(effective_retention_deadline_ms(&trusted, &public, 3_651).is_err());

    let invalid_clock = ConflictCandidate {
        value: [1; 32],
        clock: HybridLogicalClock::new("bad node", 1),
    };
    let valid_clock = ConflictCandidate {
        value: [2; 32],
        clock: HybridLogicalClock::new("phone", 1),
    };
    assert!(conflict_timeline(invalid_clock, valid_clock).is_err());

    let selection = ReplaySelection {
        item_ids: BTreeSet::from(["bad id".into()]),
        maximum_items: 1,
        ..ReplaySelection::default()
    };
    assert!(plan_selective_replay(&trusted, &[public], &selection).is_err());
    assert!(!format!("{selection:?}").contains("bad id"));

    let mut invalid_policy = trusted.clone();
    invalid_policy.device_id = "bad device".into();
    invalid_policy.clipboard_write_allowed = true;
    assert!(!invalid_policy.allows_live_clipboard_write());

    let invalid_travel = TravelMode {
        enabled: false,
        enabled_at_ms: 100,
        expires_at_ms: None,
        retention_hours: 0,
    };
    assert!(!invalid_travel.sync_allowed(150));
    assert!(!invalid_travel.sensitive_visible(150));
    assert!(invalid_travel.active(150).is_err());
}
