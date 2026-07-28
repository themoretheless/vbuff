use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use ed25519_dalek::SigningKey;
use url::Url;

use super::*;
use crate::{
    ActionPermissionRequest, PluginCapability, PluginManifest, protocol::PROTOCOL_VERSION,
};

fn manifest() -> PluginManifest {
    PluginManifest {
        id: "dev.vbuff.sample".into(),
        name: "Sample".into(),
        version: "1.0.0".into(),
        protocol_version: PROTOCOL_VERSION,
        executable_path: "bin/plugin".into(),
        requested_capabilities: BTreeSet::from([
            PluginCapability::ReadClipContent,
            PluginCapability::Network,
        ]),
        network_hosts: BTreeSet::from(["api.example.com".into()]),
        file_paths: BTreeSet::new(),
        process_commands: BTreeSet::new(),
    }
}

fn request() -> ActionPermissionRequest {
    ActionPermissionRequest {
        action_id: "summarize".into(),
        required: BTreeSet::from([PluginCapability::ReadClipContent, PluginCapability::Network]),
        network_hosts: BTreeSet::from(["api.example.com".into()]),
        file_paths: BTreeSet::new(),
        process_commands: BTreeSet::new(),
    }
}

#[test]
fn test_evidence_is_hash_only_and_fails_on_capability_escape() {
    let case = PluginTestCase {
        schema: 1,
        case_id: "summary-basic".into(),
        manifest_hash: [1; 32],
        action_id: "summarize".into(),
        fixture_hash: [2; 32],
        expected_output_hash: [3; 32],
        allowed_capabilities: BTreeSet::from([PluginCapability::ReadClipContent]),
        timeout_ms: 1_000,
    };
    let observation = PluginTestObservation {
        case_id: case.case_id.clone(),
        output_hash: [3; 32],
        attempted_capabilities: BTreeSet::from([
            PluginCapability::ReadClipContent,
            PluginCapability::Network,
        ]),
        duration_ms: 10,
        panicked: false,
    };
    let verdict = case.evaluate(&observation).unwrap();
    assert!(!verdict.passed);
    assert!(!verdict.capability_scope_respected);
    let json = serde_json::to_string(&case).unwrap();
    assert!(!json.contains("\"payload\""));
    assert!(!json.contains("\"fixture_bytes\""));
    assert!(!json.contains("\"input\""));
}

#[test]
fn signed_action_bundle_detects_permission_edits() {
    let manifest = manifest();
    let mut bundle = ActionBundle {
        schema: 1,
        bundle_id: "sample-actions".into(),
        plugin_id: manifest.id.clone(),
        plugin_version: manifest.version.clone(),
        manifest_hash: manifest.hash().unwrap(),
        actions: BTreeMap::from([("summarize".into(), request())]),
    };
    let key = SigningKey::from_bytes(&[7; 32]);
    let signed = SignedActionBundle::sign(&bundle, &manifest, &key).unwrap();
    signed.verify(&bundle, &manifest).unwrap();
    let original_bundle = bundle.clone();
    bundle
        .actions
        .get_mut("summarize")
        .unwrap()
        .network_hosts
        .clear();
    assert!(signed.verify(&bundle, &manifest).is_err());

    let mut changed_manifest = manifest;
    changed_manifest.name = "Changed without a version bump".into();
    assert!(signed.verify(&original_bundle, &changed_manifest).is_err());
}

#[test]
fn network_fetch_is_exact_host_https_and_drops_sensitive_url_parts() {
    let policy = NetworkFetchPolicy::new(1_024).unwrap();
    let manifest = manifest();
    let request = request();
    let granted = policy
        .authorize(
            &manifest,
            &request,
            &Url::parse("https://api.example.com/v1?q=secret").unwrap(),
        )
        .unwrap();
    assert_eq!(granted.host, "api.example.com");
    assert_eq!(granted.port, 443);
    assert!(granted.allows_resolved_address(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
    assert!(granted.allows_resolved_address(IpAddr::V6(
        "2606:4700:4700::1111".parse::<Ipv6Addr>().unwrap()
    )));
    for address in [
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
        IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1)),
        "::ffff:127.0.0.1".parse().unwrap(),
        "fc00::1".parse().unwrap(),
        "fe80::1".parse().unwrap(),
    ] {
        assert!(!granted.allows_resolved_address(address));
    }
    assert!(!format!("{granted:?}").contains("secret"));
    for denied in [
        "http://api.example.com",
        "https://api.example.com:8443",
        "https://127.0.0.1",
        "https://other.example.com",
    ] {
        assert!(
            policy
                .authorize(&manifest, &request, &Url::parse(denied).unwrap())
                .is_err()
        );
    }
}

#[test]
fn marketplace_permissions_must_exactly_match_manifest() {
    let manifest = manifest();
    let mut metadata = MarketplaceMetadata {
        schema: 1,
        plugin_id: manifest.id.clone(),
        display_name: manifest.name.clone(),
        summary: "Produces a bounded summary through one declared endpoint.".into(),
        license: "MIT".into(),
        categories: BTreeSet::from([MarketplaceCategory::Productivity]),
        declared_capabilities: manifest.requested_capabilities.clone(),
        minimum_protocol: PROTOCOL_VERSION,
        maximum_protocol: PROTOCOL_VERSION,
        documentation_url: Some("https://example.com/docs".into()),
        examples: vec![MarketplaceExample {
            title: "Summarize a note".into(),
            action_id: "summarize".into(),
        }],
    };
    metadata.validate(&manifest).unwrap();
    metadata
        .declared_capabilities
        .remove(&PluginCapability::Network);
    assert!(metadata.validate(&manifest).is_err());
    metadata.declared_capabilities = manifest.requested_capabilities.clone();
    metadata.documentation_url = Some("https://127.0.0.1/docs".into());
    assert!(metadata.validate(&manifest).is_err());
}

#[test]
fn panic_disables_only_the_faulting_plugin_and_keeps_content_out() {
    let mut supervisor = PluginSupervisor::default();
    supervisor.register("dev.vbuff.one".into()).unwrap();
    supervisor.register("dev.vbuff.two".into()).unwrap();
    let report = supervisor
        .record_panic("dev.vbuff.one", "summarize", 42)
        .unwrap();
    assert!(!supervisor.can_run("dev.vbuff.one"));
    assert!(supervisor.can_run("dev.vbuff.two"));
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("payload"));
    assert!(!json.contains("content"));
}
