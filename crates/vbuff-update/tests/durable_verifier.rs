//! Integration tests for durable verifier state: the anti-replay watermark,
//! revocations, and key rotations must survive process restarts, and corrupt
//! state must fail closed rather than silently reset.

use ed25519_dalek::SigningKey;
use semver::Version;
use vbuff_update::{
    Artifact, KeyRotation, SignedUpdateManifest, TrustedKey, UpdateError, UpdateKeyring,
    UpdateManifest, UpdateVerifier,
};

fn manifest(sequence: u64, version: &str) -> UpdateManifest {
    UpdateManifest {
        schema: 1,
        sequence,
        version: Version::parse(version).unwrap(),
        minimum_client: Version::parse("0.1.0").unwrap(),
        published_at_ms: 100,
        rollout_percent: 100,
        artifacts: vec![Artifact {
            target: "aarch64-apple-darwin".into(),
            url: "https://releases.vbuff.dev/vbuff".into(),
            sha256: [3; 32],
            byte_size: 42,
        }],
        next_key: None,
    }
}

fn bootstrap_keyring(key: &SigningKey) -> UpdateKeyring {
    let mut keyring = UpdateKeyring::default();
    keyring
        .trust(
            "release-1",
            TrustedKey {
                public_key: key.verifying_key().to_bytes(),
                activates_at_sequence: 1,
                revoked_at_sequence: None,
            },
        )
        .unwrap();
    keyring
}

#[test]
fn watermark_survives_restart_and_blocks_replay() {
    let key = SigningKey::from_bytes(&[7; 32]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("verifier-state.json");
    let current = Version::parse("0.1.0").unwrap();

    let mut verifier = UpdateVerifier::load(&path, || bootstrap_keyring(&key)).unwrap();
    let release = SignedUpdateManifest::sign("release-1", manifest(10, "0.2.0"), &key).unwrap();
    verifier.verify(&release, &current, b"install").unwrap();
    verifier.store(&path).unwrap();
    drop(verifier);

    // Simulated restart: the verifier is rebuilt from disk only.
    let mut verifier = UpdateVerifier::load(&path, || bootstrap_keyring(&key)).unwrap();
    assert_eq!(verifier.highest_accepted_sequence(), 10);
    assert_eq!(
        verifier.verify(&release, &current, b"install"),
        Err(UpdateError::DowngradeOrReplay)
    );
    let older = SignedUpdateManifest::sign("release-1", manifest(9, "0.1.5"), &key).unwrap();
    assert_eq!(
        verifier.verify(&older, &current, b"install"),
        Err(UpdateError::DowngradeOrReplay)
    );
    let newer = SignedUpdateManifest::sign("release-1", manifest(11, "0.3.0"), &key).unwrap();
    assert!(verifier.verify(&newer, &current, b"install").is_ok());
}

#[test]
fn revocation_survives_store_and_load() {
    let key = SigningKey::from_bytes(&[7; 32]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("verifier-state.json");

    let mut keyring = bootstrap_keyring(&key);
    keyring.revoke("release-1", 5).unwrap();
    let verifier = UpdateVerifier::new(keyring, 0);
    verifier.store(&path).unwrap();
    drop(verifier);

    let mut verifier = UpdateVerifier::load(&path, || bootstrap_keyring(&key)).unwrap();
    let release = SignedUpdateManifest::sign("release-1", manifest(6, "0.2.0"), &key).unwrap();
    assert_eq!(
        verifier.verify(&release, &Version::parse("0.1.0").unwrap(), b"install"),
        Err(UpdateError::UntrustedKey)
    );
}

#[test]
fn corrupt_state_file_fails_closed() {
    let key = SigningKey::from_bytes(&[7; 32]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("verifier-state.json");
    std::fs::write(&path, b"{ not valid state").unwrap();
    // No silent reset to sequence 0: a corrupt file is a hard error.
    assert!(UpdateVerifier::load(&path, || bootstrap_keyring(&key)).is_err());
}

#[test]
fn missing_state_file_bootstraps_fresh_keyring() {
    let key = SigningKey::from_bytes(&[7; 32]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("verifier-state.json");
    let mut verifier = UpdateVerifier::load(&path, || bootstrap_keyring(&key)).unwrap();
    assert_eq!(verifier.highest_accepted_sequence(), 0);
    let release = SignedUpdateManifest::sign("release-1", manifest(1, "0.2.0"), &key).unwrap();
    assert!(
        verifier
            .verify(&release, &Version::parse("0.1.0").unwrap(), b"install")
            .is_ok()
    );
}

#[test]
fn rotation_survives_restarts_and_retires_old_key() {
    let first = SigningKey::from_bytes(&[1; 32]);
    let second = SigningKey::from_bytes(&[2; 32]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("verifier-state.json");
    let current = Version::parse("0.1.0").unwrap();

    // Boot: accept release 10 signed by the first key.
    let mut verifier = UpdateVerifier::load(&path, || bootstrap_keyring(&first)).unwrap();
    let release = SignedUpdateManifest::sign("release-1", manifest(10, "0.2.0"), &first).unwrap();
    verifier.verify(&release, &current, b"install").unwrap();
    verifier.store(&path).unwrap();
    drop(verifier);

    // Restart 1: accept a schema-2 rotation at sequence 11.
    let mut verifier = UpdateVerifier::load(&path, || bootstrap_keyring(&first)).unwrap();
    let mut rotating = manifest(11, "0.3.0");
    rotating.schema = 2;
    rotating.next_key = Some(KeyRotation::confirmed("release-2", &second, 12, 11).unwrap());
    let rotating = SignedUpdateManifest::sign("release-1", rotating, &first).unwrap();
    verifier.verify(&rotating, &current, b"install").unwrap();
    verifier.store(&path).unwrap();
    drop(verifier);

    // Restart 2: the new key signs from its activation sequence; the retired
    // key is rejected even with a higher sequence.
    let mut verifier = UpdateVerifier::load(&path, || bootstrap_keyring(&first)).unwrap();
    let by_new = SignedUpdateManifest::sign("release-2", manifest(12, "0.4.0"), &second).unwrap();
    assert!(verifier.verify(&by_new, &current, b"install").is_ok());
    let by_old = SignedUpdateManifest::sign("release-1", manifest(13, "0.5.0"), &first).unwrap();
    assert_eq!(
        verifier.verify(&by_old, &current, b"install"),
        Err(UpdateError::UntrustedKey)
    );
}
