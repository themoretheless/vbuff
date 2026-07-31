//! Hash-chained device membership with per-entry Ed25519 signatures,
//! whole-set SAS, and epoch revocation.
//!
//! Single-writer model: the log opens with the owner device's self-add
//! (entry 0), and only that owner may author later entries. Every entry
//! carries an Ed25519 signature over its hash made with the author's
//! registered signing key, so authorship cannot be forged by rewriting
//! `added_by`. Concurrent multi-writer membership is intentionally not
//! supported: strict clock monotonicity fails closed instead of merging
//! competing heads, and stays in place until a CRDT-based design lands.
//! Ownership transfer is a future signed operation.

use std::collections::BTreeMap;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, StaticSecret};

use vbuff_types::validation::is_valid_identifier;

use crate::clock::HybridLogicalClock;
use crate::crypto::{SealedEnvelope, seal_to};
use crate::{Result, SyncError};

/// Maximum byte length of device, author, and clock-node identifiers.
const MAX_DEVICE_ID_BYTES: usize = 128;
/// Fail-closed bound on the number of membership entries.
const MAX_MEMBERSHIP_ENTRIES: usize = 1_024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceMember {
    pub device_id: String,
    /// x25519 key used to seal group keys to this device.
    pub public_key: [u8; 32],
    /// Ed25519 verifying key authorizing entries authored by this device.
    pub signing_key: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipAction {
    Add(DeviceMember),
    Revoke { device_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembershipEntry {
    pub action: MembershipAction,
    pub added_by: String,
    pub clock: HybridLogicalClock,
    pub previous_hash: [u8; 32],
    pub hash: [u8; 32],
    /// Ed25519 signature over `hash` by the author's registered signing key.
    #[serde(with = "serde_signature")]
    pub signature: [u8; 64],
}

/// Serde support for the fixed-width signature (serde derives arrays only up
/// to 32 elements); deserialization fails closed on any other length.
mod serde_signature {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(value: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error> {
        value.as_slice().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 64], D::Error> {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        <[u8; 64]>::try_from(bytes.as_slice())
            .map_err(|_| serde::de::Error::custom("membership signature must be 64 bytes"))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembershipLog {
    pub entries: Vec<MembershipEntry>,
}

impl MembershipLog {
    /// Append an entry authored by `added_by` and signed with `author_key`.
    ///
    /// Fails closed: the log is bounded, identifiers are validated, only the
    /// owner may author past entry 0, and `author_key` must match the signing
    /// key registered for the author (the self-declared key for entry 0).
    pub fn append(
        &mut self,
        action: MembershipAction,
        added_by: impl Into<String>,
        clock: HybridLogicalClock,
        author_key: &SigningKey,
    ) -> Result<[u8; 32]> {
        if self.entries.len() >= MAX_MEMBERSHIP_ENTRIES {
            return Err(SyncError::Invalid("membership log is full".into()));
        }
        let added_by = added_by.into();
        let active = self.active_members();
        let owner = self
            .entries
            .first()
            .map_or_else(|| added_by.clone(), |entry| entry.added_by.clone());
        validate_entry_semantics(
            self.entries.is_empty(),
            &owner,
            &active,
            &action,
            &added_by,
            &clock,
            self.entries.last().map(|entry| &entry.clock),
        )?;
        let expected_signing_key = if self.entries.is_empty() {
            match &action {
                MembershipAction::Add(member) => member.signing_key,
                MembershipAction::Revoke { .. } => {
                    return Err(SyncError::Invalid(
                        "membership log must start by adding its owner".into(),
                    ));
                }
            }
        } else {
            match active.get(&added_by) {
                Some(member) => member.signing_key,
                None => {
                    return Err(SyncError::Invalid(
                        "membership change was not authorized by an active device".into(),
                    ));
                }
            }
        };
        if author_key.verifying_key().to_bytes() != expected_signing_key {
            return Err(SyncError::Invalid(
                "membership author key does not match the registered signing key".into(),
            ));
        }
        let previous_hash = self.head();
        let hash = entry_hash(&action, &added_by, &clock, &previous_hash)?;
        let signature = author_key.sign(&hash).to_bytes();
        self.entries.push(MembershipEntry {
            action,
            added_by,
            clock,
            previous_hash,
            hash,
            signature,
        });
        Ok(hash)
    }

    pub fn head(&self) -> [u8; 32] {
        self.entries.last().map_or([0; 32], |entry| entry.hash)
    }

    /// Re-derive and check the whole chain: bound, hashes, entry semantics,
    /// and each entry's signature against the replayed active set.
    pub fn verify(&self) -> Result<()> {
        if self.entries.len() > MAX_MEMBERSHIP_ENTRIES {
            return Err(SyncError::Invalid(
                "membership log exceeds the entry limit".into(),
            ));
        }
        let owner = self
            .entries
            .first()
            .map_or_else(String::new, |entry| entry.added_by.clone());
        let mut previous = [0_u8; 32];
        let mut previous_clock = None;
        let mut active = BTreeMap::new();
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.previous_hash != previous
                || entry.hash
                    != entry_hash(
                        &entry.action,
                        &entry.added_by,
                        &entry.clock,
                        &entry.previous_hash,
                    )?
            {
                return Err(SyncError::Invalid("membership hash chain is broken".into()));
            }
            validate_entry_semantics(
                index == 0,
                &owner,
                &active,
                &entry.action,
                &entry.added_by,
                &entry.clock,
                previous_clock,
            )?;
            let signing_key = if index == 0 {
                match &entry.action {
                    MembershipAction::Add(member) => member.signing_key,
                    MembershipAction::Revoke { .. } => {
                        return Err(SyncError::Invalid(
                            "membership log must start by adding its owner".into(),
                        ));
                    }
                }
            } else {
                match active.get(&entry.added_by) {
                    Some(member) => member.signing_key,
                    None => {
                        return Err(SyncError::Invalid(
                            "membership change was not authorized by an active device".into(),
                        ));
                    }
                }
            };
            let key = VerifyingKey::from_bytes(&signing_key).map_err(|_| SyncError::Crypto)?;
            key.verify(&entry.hash, &Signature::from_bytes(&entry.signature))
                .map_err(|_| SyncError::Crypto)?;
            apply_action(&mut active, &entry.action);
            previous = entry.hash;
            previous_clock = Some(&entry.clock);
        }
        Ok(())
    }

    /// Replay the log into the active device set keyed by device ID.
    pub fn active_members(&self) -> BTreeMap<String, DeviceMember> {
        let mut members = BTreeMap::new();
        for entry in &self.entries {
            apply_action(&mut members, &entry.action);
        }
        members
    }

    /// Twenty-digit SAS committing to the whole membership head and both
    /// pairing keys, rendered as four groups of five digits.
    ///
    /// The BLAKE3 digest is reduced modulo 10^20, giving log2(10^20) ≈ 66.4
    /// bits of entropy — above the 60-bit floor expected for interactive
    /// pairing ceremonies.
    pub fn sas(&self, first_key: &[u8; 32], second_key: &[u8; 32]) -> String {
        let (left, right) = if first_key <= second_key {
            (first_key, second_key)
        } else {
            (second_key, first_key)
        };
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"vbuff-membership-sas-v2");
        hasher.update(&self.head());
        hasher.update(left);
        hasher.update(right);
        let bytes = hasher.finalize();
        let value = u128::from_le_bytes(bytes.as_bytes()[0..16].try_into().unwrap())
            % 10_000_000_000_000_000_000_u128;
        let digits = format!("{value:020}");
        format!(
            "{}-{}-{}-{}",
            &digits[0..5],
            &digits[5..10],
            &digits[10..15],
            &digits[15..20]
        )
    }
}

fn validate_entry_semantics(
    first: bool,
    owner: &str,
    active: &BTreeMap<String, DeviceMember>,
    action: &MembershipAction,
    added_by: &str,
    clock: &HybridLogicalClock,
    previous_clock: Option<&HybridLogicalClock>,
) -> Result<()> {
    if !is_valid_identifier(added_by, MAX_DEVICE_ID_BYTES) {
        return Err(SyncError::Invalid(
            "membership author identifier is invalid".into(),
        ));
    }
    if !is_valid_identifier(&clock.node_id, MAX_DEVICE_ID_BYTES) {
        return Err(SyncError::Invalid(
            "membership clock node identifier is invalid".into(),
        ));
    }
    if first {
        let MembershipAction::Add(member) = action else {
            return Err(SyncError::Invalid(
                "membership log must start by adding its owner".into(),
            ));
        };
        if member.device_id != added_by {
            return Err(SyncError::Invalid(
                "first membership entry must be self-added".into(),
            ));
        }
    } else {
        if added_by != owner {
            return Err(SyncError::Invalid(
                "membership entries after the first are authored only by the owner".into(),
            ));
        }
        if !active.contains_key(added_by) {
            return Err(SyncError::Invalid(
                "membership change was not authorized by an active device".into(),
            ));
        }
    }
    if previous_clock.is_some_and(|previous| clock <= previous) {
        return Err(SyncError::Invalid(
            "membership clock must advance monotonically".into(),
        ));
    }
    match action {
        MembershipAction::Add(member) => {
            if !is_valid_identifier(&member.device_id, MAX_DEVICE_ID_BYTES) {
                return Err(SyncError::Invalid("device identifier is invalid".into()));
            }
            if active.contains_key(&member.device_id) {
                return Err(SyncError::Invalid("device is already active".into()));
            }
            validate_public_key(&member.public_key)
        }
        MembershipAction::Revoke { device_id } => {
            if !is_valid_identifier(device_id, MAX_DEVICE_ID_BYTES) {
                return Err(SyncError::Invalid("device identifier is invalid".into()));
            }
            if !active.contains_key(device_id) {
                return Err(SyncError::Invalid(
                    "cannot revoke a device that is not active".into(),
                ));
            }
            Ok(())
        }
    }
}

fn apply_action(active: &mut BTreeMap<String, DeviceMember>, action: &MembershipAction) {
    match action {
        MembershipAction::Add(member) => {
            active.insert(member.device_id.clone(), member.clone());
        }
        MembershipAction::Revoke { device_id } => {
            active.remove(device_id);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochTransition {
    pub epoch: u64,
    pub revoked_device: String,
    pub key_commitment: [u8; 32],
    pub wrapped_group_keys: BTreeMap<String, SealedEnvelope>,
}

/// Revoke a device and wrap the fresh group key to every remaining member.
///
/// Staging semantics are preserved: the revocation is applied to a cloned
/// log and published only after the transition is fully built, so any
/// failure (including a wrong `author_key`) leaves `log` untouched.
pub fn revoke_and_rekey(
    log: &mut MembershipLog,
    revoked_device: &str,
    added_by: &str,
    clock: HybridLogicalClock,
    current_epoch: u64,
    new_group_key: &[u8; 32],
    author_key: &SigningKey,
) -> Result<EpochTransition> {
    log.verify()?;
    if !log.active_members().contains_key(revoked_device) {
        return Err(SyncError::Invalid("device is not an active member".into()));
    }
    let mut staged = log.clone();
    staged.append(
        MembershipAction::Revoke {
            device_id: revoked_device.into(),
        },
        added_by,
        clock,
        author_key,
    )?;
    let epoch = current_epoch
        .checked_add(1)
        .ok_or_else(|| SyncError::Invalid("membership epoch exhausted".into()))?;
    let aad = format!("vbuff-group-epoch-{epoch}");
    let wrapped_group_keys = staged
        .active_members()
        .into_iter()
        .map(|(device, member)| {
            Ok((
                device,
                seal_to(&member.public_key, new_group_key, aad.as_bytes())?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let transition = EpochTransition {
        epoch,
        revoked_device: revoked_device.into(),
        key_commitment: *blake3::hash(new_group_key).as_bytes(),
        wrapped_group_keys,
    };
    *log = staged;
    Ok(transition)
}

fn validate_public_key(bytes: &[u8; 32]) -> Result<()> {
    let probe = StaticSecret::from([0xA5; 32]);
    let shared = probe.diffie_hellman(&PublicKey::from(*bytes));
    if !shared.was_contributory() {
        return Err(SyncError::Invalid(
            "non-contributory device public key".into(),
        ));
    }
    Ok(())
}

fn entry_hash(
    action: &MembershipAction,
    added_by: &str,
    clock: &HybridLogicalClock,
    previous_hash: &[u8; 32],
) -> Result<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vbuff-membership-entry-v2");
    hasher.update(previous_hash);
    hasher.update(&serde_json::to_vec(&(action, added_by, clock))?);
    Ok(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::open_sealed;
    use x25519_dalek::{PublicKey, StaticSecret};

    fn member_parts(
        device_id: &str,
        secret_seed: u8,
        signing_seed: u8,
    ) -> (StaticSecret, SigningKey, DeviceMember) {
        let secret = StaticSecret::from([secret_seed; 32]);
        let signing = SigningKey::from_bytes(&[signing_seed; 32]);
        let member = DeviceMember {
            device_id: device_id.into(),
            public_key: PublicKey::from(&secret).to_bytes(),
            signing_key: signing.verifying_key().to_bytes(),
        };
        (secret, signing, member)
    }

    fn two_device_log() -> (MembershipLog, SigningKey, SigningKey) {
        let (_, a_key, a_member) = member_parts("a", 21, 22);
        let (_, b_key, b_member) = member_parts("b", 23, 24);
        let mut log = MembershipLog::default();
        log.append(
            MembershipAction::Add(a_member),
            "a",
            HybridLogicalClock::new("a", 1),
            &a_key,
        )
        .unwrap();
        log.append(
            MembershipAction::Add(b_member),
            "a",
            HybridLogicalClock::new("a", 2),
            &a_key,
        )
        .unwrap();
        (log, a_key, b_key)
    }

    #[test]
    fn signed_entries_verify_and_forged_author_is_rejected() {
        let (log, _, _) = two_device_log();
        log.verify().unwrap();

        // Forged author with a recomputed hash but no matching signature.
        let mut forged_author = log.clone();
        let entry = &mut forged_author.entries[1];
        entry.added_by = "ghost".into();
        entry.hash = entry_hash(
            &entry.action,
            &entry.added_by,
            &entry.clock,
            &entry.previous_hash,
        )
        .unwrap();
        assert!(forged_author.verify().is_err());

        // Forged content under the real author still fails the signature.
        let mut forged_action = log;
        let (_, _, evil) = member_parts("evil", 25, 26);
        let entry = &mut forged_action.entries[1];
        entry.action = MembershipAction::Add(evil);
        entry.hash = entry_hash(
            &entry.action,
            &entry.added_by,
            &entry.clock,
            &entry.previous_hash,
        )
        .unwrap();
        assert!(forged_action.verify().is_err());
    }

    #[test]
    fn wrong_signing_key_is_rejected() {
        let wrong = SigningKey::from_bytes(&[99; 32]);

        // Entry 0: the key must match the self-declared signing key.
        let (_, _, member) = member_parts("a", 31, 32);
        let mut fresh = MembershipLog::default();
        assert!(
            fresh
                .append(
                    MembershipAction::Add(member),
                    "a",
                    HybridLogicalClock::new("a", 1),
                    &wrong,
                )
                .is_err()
        );
        assert!(fresh.entries.is_empty());

        // Later entries: the owner's registered key is required.
        let (mut log, _, _) = two_device_log();
        let before = log.clone();
        let (_, _, c_member) = member_parts("c", 33, 34);
        assert!(
            log.append(
                MembershipAction::Add(c_member),
                "a",
                HybridLogicalClock::new("a", 3),
                &wrong,
            )
            .is_err()
        );
        assert_eq!(log, before);
    }

    #[test]
    fn revoked_device_cannot_author_later_entries() {
        let (mut log, a_key, b_key) = two_device_log();
        log.append(
            MembershipAction::Revoke {
                device_id: "b".into(),
            },
            "a",
            HybridLogicalClock::new("a", 3),
            &a_key,
        )
        .unwrap();
        log.verify().unwrap();

        let before = log.clone();
        let (_, _, c_member) = member_parts("c", 35, 36);
        assert!(
            log.append(
                MembershipAction::Add(c_member),
                "b",
                HybridLogicalClock::new("b", 4),
                &b_key,
            )
            .is_err()
        );
        assert_eq!(log, before);
        log.verify().unwrap();
    }

    #[test]
    fn non_owner_active_device_cannot_author() {
        let (mut log, _, b_key) = two_device_log();
        let before = log.clone();
        let (_, _, c_member) = member_parts("c", 37, 38);
        // "b" is active and signs with its own key, but is not the owner.
        assert!(
            log.append(
                MembershipAction::Add(c_member),
                "b",
                HybridLogicalClock::new("b", 3),
                &b_key,
            )
            .is_err()
        );
        assert_eq!(log, before);
    }

    #[test]
    fn sas_has_at_least_60_bits_and_is_order_independent() {
        let (mut log, a_key, _) = two_device_log();
        let sas = log.sas(&[2; 32], &[3; 32]);
        // Four groups of five digits: log2(10^20) ≈ 66.4 bits ≥ 60 bits.
        assert_eq!(sas.len(), 23);
        assert_eq!(sas.split('-').count(), 4);
        for group in sas.split('-') {
            assert_eq!(group.len(), 5);
            assert!(group.bytes().all(|byte| byte.is_ascii_digit()));
        }
        assert_eq!(sas, log.sas(&[2; 32], &[3; 32]));
        assert_eq!(sas, log.sas(&[3; 32], &[2; 32]));

        let (_, _, c_member) = member_parts("c", 39, 40);
        log.append(
            MembershipAction::Add(c_member),
            "a",
            HybridLogicalClock::new("a", 3),
            &a_key,
        )
        .unwrap();
        assert_ne!(sas, log.sas(&[2; 32], &[3; 32]));
    }

    #[test]
    fn identifiers_are_validated() {
        let (mut log, a_key, _) = two_device_log();
        let overlong = "x".repeat(MAX_DEVICE_ID_BYTES + 1);
        for bad in ["", "has space", "девайс", "💾", overlong.as_str()] {
            let clock = HybridLogicalClock::new("a", 10);
            let (_, _, bad_member) = member_parts(bad, 41, 42);
            assert!(
                log.append(
                    MembershipAction::Add(bad_member),
                    "a",
                    clock.clone(),
                    &a_key,
                )
                .is_err()
            );
            assert!(
                log.append(
                    MembershipAction::Revoke {
                        device_id: bad.into(),
                    },
                    "a",
                    clock.clone(),
                    &a_key,
                )
                .is_err()
            );
            let (_, _, c_member) = member_parts("c", 43, 44);
            assert!(
                log.append(
                    MembershipAction::Add(c_member.clone()),
                    bad,
                    clock.clone(),
                    &a_key
                )
                .is_err()
            );
            assert!(
                log.append(
                    MembershipAction::Add(c_member),
                    "a",
                    HybridLogicalClock::new(bad, 10),
                    &a_key,
                )
                .is_err()
            );
        }
        assert_eq!(log.entries.len(), 2);
        log.verify().unwrap();
    }

    #[test]
    fn log_is_bounded() {
        let (_, a_key, a_member) = member_parts("a", 51, 52);
        let mut log = MembershipLog::default();
        log.append(
            MembershipAction::Add(a_member),
            "a",
            HybridLogicalClock::new("a", 1),
            &a_key,
        )
        .unwrap();
        for index in 0..(MAX_MEMBERSHIP_ENTRIES - 1) {
            let mut secret_bytes = [0_u8; 32];
            secret_bytes[..8].copy_from_slice(&(index as u64 + 1).to_le_bytes());
            let secret = StaticSecret::from(secret_bytes);
            let mut key_bytes = [0_u8; 32];
            key_bytes[..8].copy_from_slice(&(index as u64 + 10_000).to_le_bytes());
            let signing = SigningKey::from_bytes(&key_bytes);
            log.append(
                MembershipAction::Add(DeviceMember {
                    device_id: format!("d{index}"),
                    public_key: PublicKey::from(&secret).to_bytes(),
                    signing_key: signing.verifying_key().to_bytes(),
                }),
                "a",
                HybridLogicalClock::new("a", index as u64 + 2),
                &a_key,
            )
            .unwrap();
        }
        assert_eq!(log.entries.len(), MAX_MEMBERSHIP_ENTRIES);
        log.verify().unwrap();

        let (_, _, extra) = member_parts("overflow", 53, 54);
        assert!(
            log.append(
                MembershipAction::Add(extra),
                "a",
                HybridLogicalClock::new("a", MAX_MEMBERSHIP_ENTRIES as u64 + 1),
                &a_key,
            )
            .is_err()
        );

        // A log constructed past the bound fails closed in verify.
        log.entries.push(log.entries[0].clone());
        assert!(log.verify().is_err());
    }

    #[test]
    fn revoke_and_rekey_requires_owner_key() {
        let (mut log, _, b_key) = two_device_log();
        let before = log.clone();
        // A non-owner author is rejected even with its own valid key.
        assert!(
            revoke_and_rekey(
                &mut log,
                "b",
                "b",
                HybridLogicalClock::new("b", 3),
                4,
                &[7; 32],
                &b_key,
            )
            .is_err()
        );
        // The owner name with someone else's key is rejected as well.
        assert!(
            revoke_and_rekey(
                &mut log,
                "b",
                "a",
                HybridLogicalClock::new("a", 3),
                4,
                &[7; 32],
                &b_key,
            )
            .is_err()
        );
        assert_eq!(log, before);
    }

    #[test]
    fn sas_commits_to_full_verified_chain() {
        let (_, a_key, a_member) = member_parts("a", 9, 10);
        let mut log = MembershipLog::default();
        log.append(
            MembershipAction::Add(a_member),
            "a",
            HybridLogicalClock::new("a", 1),
            &a_key,
        )
        .unwrap();
        log.verify().unwrap();
        assert_eq!(log.sas(&[2; 32], &[3; 32]), log.sas(&[3; 32], &[2; 32]));
        log.entries[0].added_by = "attacker".into();
        assert!(log.verify().is_err());
    }

    #[test]
    fn verification_replays_authorization_even_if_hashes_are_recomputed() {
        let (mut log, _, _) = two_device_log();
        let entry = &mut log.entries[1];
        entry.added_by = "ghost".into();
        entry.hash = entry_hash(
            &entry.action,
            &entry.added_by,
            &entry.clock,
            &entry.previous_hash,
        )
        .unwrap();
        assert!(log.verify().is_err());
    }

    #[test]
    fn revoked_device_receives_no_new_epoch_key() {
        let (a_secret, a_key, a_member) = member_parts("a", 11, 61);
        let (_, _, b_member) = member_parts("b", 12, 62);
        let mut log = MembershipLog::default();
        log.append(
            MembershipAction::Add(a_member),
            "a",
            HybridLogicalClock::new("a", 1),
            &a_key,
        )
        .unwrap();
        log.append(
            MembershipAction::Add(b_member),
            "a",
            HybridLogicalClock::new("a", 2),
            &a_key,
        )
        .unwrap();
        let transition = revoke_and_rekey(
            &mut log,
            "b",
            "a",
            HybridLogicalClock::new("a", 3),
            4,
            &[99; 32],
            &a_key,
        )
        .unwrap();
        assert_eq!(transition.epoch, 5);
        assert!(!transition.wrapped_group_keys.contains_key("b"));
        let aad = b"vbuff-group-epoch-5";
        assert_eq!(
            open_sealed(&a_secret, &transition.wrapped_group_keys["a"], aad).unwrap(),
            [99; 32]
        );
    }

    #[test]
    fn membership_rejects_low_order_keys_and_unauthorized_changes() {
        let (_, a_key, mut low_order) = member_parts("a", 10, 60);
        let mut log = MembershipLog::default();
        low_order.public_key = [0; 32];
        assert!(
            log.append(
                MembershipAction::Add(low_order),
                "a",
                HybridLogicalClock::new("a", 1),
                &a_key,
            )
            .is_err()
        );

        let (_, _, member) = member_parts("a", 10, 60);
        log.append(
            MembershipAction::Add(member),
            "a",
            HybridLogicalClock::new("a", 1),
            &a_key,
        )
        .unwrap();
        assert!(
            log.append(
                MembershipAction::Revoke {
                    device_id: "a".into(),
                },
                "attacker",
                HybridLogicalClock::new("attacker", 2),
                &a_key,
            )
            .is_err()
        );
    }

    #[test]
    fn failed_epoch_transition_does_not_publish_the_revocation() {
        let (_, a_key, a_member) = member_parts("a", 41, 63);
        let (_, _, b_member) = member_parts("b", 42, 64);
        let mut log = MembershipLog::default();
        log.append(
            MembershipAction::Add(a_member),
            "a",
            HybridLogicalClock::new("a", 1),
            &a_key,
        )
        .unwrap();
        log.append(
            MembershipAction::Add(b_member),
            "a",
            HybridLogicalClock::new("a", 2),
            &a_key,
        )
        .unwrap();
        let before = log.clone();

        assert!(
            revoke_and_rekey(
                &mut log,
                "b",
                "a",
                HybridLogicalClock::new("a", 3),
                u64::MAX,
                &[7; 32],
                &a_key,
            )
            .is_err()
        );
        assert_eq!(log, before);
    }
}
