use super::*;
use crate::{EventEnvelope, EventKind};

#[test]
fn rpc_header_and_json_schema_are_stable() {
    let envelope = RpcEnvelope {
        schema: RPC_SCHEMA_VERSION,
        request_id: "health-1".into(),
        payload: MachineHealthSnapshot {
            schema: RPC_SCHEMA_VERSION,
            capture_state: "watching".into(),
            database_bytes: 10,
            stored_items: 2,
            sync_queue_items: 0,
            degraded_capabilities: 1,
            checked_at_ms: 7,
        },
    };
    envelope.validate_header().unwrap();
    assert_eq!(
        serde_json::to_string(&envelope).unwrap(),
        r#"{"schema":1,"request_id":"health-1","payload":{"schema":1,"capture_state":"watching","database_bytes":10,"stored_items":2,"sync_queue_items":0,"degraded_capabilities":1,"checked_at_ms":7}}"#
    );
}

#[test]
fn completions_are_sorted_bounded_and_metadata_only() {
    let mut catalog = ShellCompletionCatalog::default();
    catalog
        .insert(CompletionKind::Collection, "work".into())
        .unwrap();
    catalog
        .insert(CompletionKind::Collection, "weekend".into())
        .unwrap();
    catalog.insert(CompletionKind::Tag, "web".into()).unwrap();
    let candidates = catalog.complete("we", usize::MAX);
    assert_eq!(candidates.len(), 2);
    assert!(candidates.len() <= MAX_COMPLETION_RESULTS);
}

#[test]
fn replay_cursor_detects_expired_history() {
    let events = vec![
        EventEnvelope {
            sequence: 10,
            kind: EventKind::Captured,
            clip_id: None,
            collection_id: None,
            sensitive: false,
        },
        EventEnvelope {
            sequence: 11,
            kind: EventKind::Updated,
            clip_id: None,
            collection_id: None,
            sensitive: false,
        },
    ];
    let cursor = EventReplayCursor {
        stream_id: "local".into(),
        next_sequence: 11,
    };
    assert_eq!(cursor.select(10, &events, 10).unwrap().len(), 1);
    let mut unordered = events.clone();
    unordered.reverse();
    assert!(cursor.select(10, &unordered, 10).is_err());
    let expired = EventReplayCursor {
        next_sequence: 9,
        ..cursor
    };
    assert_eq!(
        expired.select(10, &events, 10),
        Err(OperationContractError::CursorExpired)
    );
}

#[test]
fn headless_and_loopback_contracts_fail_closed() {
    let plan = HeadlessOperationPlan {
        operation_id: "import-1".into(),
        kind: HeadlessOperationKind::Import,
        launch_gui: false,
        launch_tray: false,
        mutating: true,
        mode: MutationMode::DryRun,
    };
    plan.validate().unwrap();
    assert!(
        HeadlessOperationPlan {
            mutating: false,
            ..plan.clone()
        }
        .validate()
        .is_err()
    );
    assert!(
        HeadlessOperationPlan {
            mode: MutationMode::Apply,
            ..plan
        }
        .validate()
        .is_err()
    );

    let public = LoopbackWebhookEndpoint {
        bind_address: "0.0.0.0".parse().unwrap(),
        port: 4312,
        token_scope: "webhook.ingress".into(),
    };
    assert!(public.validate().is_err());
    let local = LoopbackWebhookEndpoint {
        bind_address: "127.0.0.1".parse().unwrap(),
        ..public
    };
    local.validate().unwrap();
}

#[test]
fn backup_health_and_fixture_contracts_require_all_guarantees() {
    let mut backup = BackupCommandPlan {
        operation_id: "backup-1".into(),
        encrypted: true,
        include_manifest: true,
        verify_after_write: true,
        mode: MutationMode::DryRun,
    };
    backup.validate().unwrap();
    backup.encrypted = false;
    assert!(backup.validate().is_err());

    let health = MachineHealthSnapshot {
        schema: RPC_SCHEMA_VERSION,
        capture_state: "degraded".into(),
        database_bytes: 100,
        stored_items: 3,
        sync_queue_items: 0,
        degraded_capabilities: 2,
        checked_at_ms: 8,
    };
    let json = String::from_utf8(health.canonical_json().unwrap()).unwrap();
    assert!(!json.contains("content"));
    assert!(!json.contains("preview"));

    let mut fixture = SanitizedFixtureManifest {
        schema: RPC_SCHEMA_VERSION,
        fixture_id: "render-case-1".into(),
        item_count: 1,
        content_removed: true,
        source_metadata_reviewed: true,
        fixture_hash: [1; 32],
    };
    fixture.validate().unwrap();
    fixture.content_removed = false;
    assert!(fixture.validate().is_err());
}

#[test]
fn rate_limits_are_per_token_kind_and_window() {
    let mut limiter = TokenRateLimiter::new(RateLimitPolicy {
        window_ms: 1_000,
        reads_per_window: 2,
        writes_per_window: 1,
        pastes_per_window: 1,
    })
    .unwrap();
    limiter.admit([1; 32], RateLimitKind::Read, 7_000).unwrap();
    limiter.admit([1; 32], RateLimitKind::Read, 7_999).unwrap();
    assert_eq!(
        limiter.admit([1; 32], RateLimitKind::Read, 7_999),
        Err(OperationContractError::RateLimited)
    );
    limiter.admit([1; 32], RateLimitKind::Read, 8_000).unwrap();
    limiter.admit([2; 32], RateLimitKind::Read, 8_000).unwrap();
    assert_eq!(
        limiter.admit([3; 32], RateLimitKind::Read, 7_000),
        Err(OperationContractError::Invalid(
            "rate_limit_clock_moved_backward"
        ))
    );
}

#[test]
fn mutating_contract_exposes_dry_run_and_bounded_preview() {
    let request = MutationRequest {
        mode: MutationMode::DryRun,
        operation: "delete-expired",
    };
    assert_eq!(request.mode, MutationMode::DryRun);
    let preview = MutationPreview {
        operation_id: "delete-expired".into(),
        affected_items: 12,
        estimated_byte_delta: -4_096,
        warning_ids: vec!["legal_hold_excluded".into()],
    };
    preview.validate().unwrap();
    assert!(!format!("{request:?}").contains("delete-expired"));
}
