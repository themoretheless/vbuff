//! Hash-chained device membership with per-entry Ed25519 signatures,
//! whole-set SAS, and epoch revocation.
//!
//! Single-writer model: the log opens with the owner device's self-add
//! (entry 0), and only that owner may author later entries. Every entry
//! carries an Ed25519 signature over its link preimage made with the
//! author's registered signing key, so authorship cannot be forged by
//! rewriting `added_by`. Concurrent multi-writer membership is intentionally
//! not supported: strict clock monotonicity fails closed instead of merging
//! competing heads, and stays in place until a CRDT-based design lands.
//! Ownership transfer is a future signed operation.
//!
//! The chain mechanics live in [`crate::chain`]; this module supplies only
//! the payload layout and the authorization rule.

use std::collections::BTreeMap;

use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, StaticSecret};

use vbuff_types::validation::is_valid_identifier;

use crate::chain::{ChainEntry, ChainLink, Preimage, SignedChain};
use crate::clock::HybridLogicalClock;
use crate::crypto::{SealedEnvelope, seal_to};
use crate::{Result, SyncError};

/// Maximum byte length of device, author, and clock-node identifiers.
const MAX_DEVICE_ID_BYTES: usize = 128;
/// Fail-closed bound on the number of membership entries.
const MAX_MEMBERSHIP_ENTRIES: usize = 1_024;
/// Preimage discriminant for [`MembershipAction::Add`].
const ACTION_ADD: u8 = 1;
/// Preimage discriminant for [`MembershipAction::Revoke`].
const ACTION_REVOKE: u8 = 2;
/// Domain for the whole-set short authentication string.
const SAS_DOMAIN: &[u8] = b"vbuff-membership-sas-v3";

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

/// The signed payload of one membership link.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MembershipEntry {
    pub action: MembershipAction,
    pub added_by: String,
    pub clock: HybridLogicalClock,
}

/// Membership log: a [`SignedChain`] of [`MembershipEntry`] payloads.
pub type MembershipLog = SignedChain<MembershipEntry>;
/// One link of a [`MembershipLog`].
pub type MembershipLink = ChainLink<MembershipEntry>;

/// State replayed from the entries preceding the one being authorized.
#[derive(Debug, Default)]
pub struct MembershipState {
    /// Author of entry 0, the only device allowed to author later entries.
    owner: Option<String>,
    active: BTreeMap<String, DeviceMember>,
    previous_clock: Option<HybridLogicalClock>,
}

impl ChainEntry for MembershipEntry {
    const DOMAIN: &'static [u8] = b"vbuff-membership-entry-v3";
    const MAX_ENTRIES: usize = MAX_MEMBERSHIP_ENTRIES;
    const LABEL: &'static str = "membership log";

    /// The log authorizes itself from its own replayed state; there is no
    /// external key directory to consult.
    type Authority = ();
    type State = MembershipState;

    fn extend_preimage(&self, preimage: &mut Preimage) {
        match &self.action {
            MembershipAction::Add(member) => {
                preimage
                    .byte(ACTION_ADD)
                    .var(member.device_id.as_bytes())
                    .fixed(&member.public_key)
                    .fixed(&member.signing_key);
            }
            MembershipAction::Revoke { device_id } => {
                preimage.byte(ACTION_REVOKE).var(device_id.as_bytes());
            }
        }
        preimage
            .var(self.added_by.as_bytes())
            .var(self.clock.node_id.as_bytes())
            .u64_be(self.clock.physical_ms)
            .u32_be(self.clock.logical);
    }

    /// The one key permitted to sign this entry.
    ///
    /// Entry 0 is self-certifying: the owner declares the signing key it
    /// will use. Later entries must be signed with the key registered for
    /// the author by the replayed active set, so revoking a device also
    /// revokes its ability to author.
    fn expected_signing_key(
        &self,
        index: usize,
        state: &MembershipState,
        _authority: &(),
    ) -> Result<[u8; 32]> {
        let first = index == 0;
        validate_entry_semantics(first, state, self)?;
        if first {
            match &self.action {
                MembershipAction::Add(member) => Ok(member.signing_key),
                MembershipAction::Revoke { .. } => Err(SyncError::Invalid(
                    "membership log must start by adding its owner".into(),
                )),
            }
        } else {
            state
                .active
                .get(&self.added_by)
                .map(|member| member.signing_key)
                .ok_or_else(|| {
                    SyncError::Invalid(
                        "membership change was not authorized by an active device".into(),
                    )
                })
        }
    }

    fn apply(&self, state: &mut MembershipState) {
        if state.owner.is_none() {
            state.owner = Some(self.added_by.clone());
        }
        apply_action(&mut state.active, &self.action);
        state.previous_clock = Some(self.clock.clone());
    }
}

impl MembershipLog {
    /// Append an entry authored by `added_by` and signed with `author_key`.
    ///
    /// Thin wrapper over [`SignedChain::append`]; all the fail-closed rules
    /// live in [`MembershipEntry::expected_signing_key`].
    pub fn append_change(
        &mut self,
        action: MembershipAction,
        added_by: impl Into<String>,
        clock: HybridLogicalClock,
        author_key: &SigningKey,
    ) -> Result<[u8; 32]> {
        self.append(
            MembershipEntry {
                action,
                added_by: added_by.into(),
                clock,
            },
            &(),
            author_key,
        )
    }

    /// Replay the log into the active device set keyed by device ID.
    #[must_use]
    pub fn active_members(&self) -> BTreeMap<String, DeviceMember> {
        self.replay().active
    }

    /// Twenty-digit SAS committing to the whole membership head and both
    /// pairing keys, rendered as four groups of five digits.
    ///
    /// The BLAKE3 digest is reduced modulo 10^20, giving log2(10^20) ≈ 66.4
    /// bits of entropy — above the 60-bit floor expected for interactive
    /// pairing ceremonies.
    ///
    /// The modulus must have exactly as many digits as the rendering: reducing
    /// modulo 10^19 while printing twenty digits pins the leading digit to
    /// zero, which costs 3.3 bits and asks the two humans to compare a digit
    /// that carries no information.
    #[must_use]
    pub fn sas(&self, first_key: &[u8; 32], second_key: &[u8; 32]) -> String {
        let (left, right) = if first_key <= second_key {
            (first_key, second_key)
        } else {
            (second_key, first_key)
        };
        let mut preimage = Preimage::new(SAS_DOMAIN);
        preimage.fixed(&self.head()).fixed(left).fixed(right);
        let bytes = blake3::hash(
            &preimage
                .finish()
                .expect("SAS preimage carries only fixed-width fields"),
        );
        let value = u128::from_le_bytes(bytes.as_bytes()[0..16].try_into().unwrap())
            % 100_000_000_000_000_000_000_u128;
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
    state: &MembershipState,
    entry: &MembershipEntry,
) -> Result<()> {
    if !is_valid_identifier(&entry.added_by, MAX_DEVICE_ID_BYTES) {
        return Err(SyncError::Invalid(
            "membership author identifier is invalid".into(),
        ));
    }
    if !is_valid_identifier(&entry.clock.node_id, MAX_DEVICE_ID_BYTES) {
        return Err(SyncError::Invalid(
            "membership clock node identifier is invalid".into(),
        ));
    }
    if first {
        let MembershipAction::Add(member) = &entry.action else {
            return Err(SyncError::Invalid(
                "membership log must start by adding its owner".into(),
            ));
        };
        if member.device_id != entry.added_by {
            return Err(SyncError::Invalid(
                "first membership entry must be self-added".into(),
            ));
        }
    } else {
        if state.owner.as_deref() != Some(entry.added_by.as_str()) {
            return Err(SyncError::Invalid(
                "membership entries after the first are authored only by the owner".into(),
            ));
        }
        if !state.active.contains_key(&entry.added_by) {
            return Err(SyncError::Invalid(
                "membership change was not authorized by an active device".into(),
            ));
        }
    }
    if state
        .previous_clock
        .as_ref()
        .is_some_and(|previous| &entry.clock <= previous)
    {
        return Err(SyncError::Invalid(
            "membership clock must advance monotonically".into(),
        ));
    }
    match &entry.action {
        MembershipAction::Add(member) => {
            if !is_valid_identifier(&member.device_id, MAX_DEVICE_ID_BYTES) {
                return Err(SyncError::Invalid("device identifier is invalid".into()));
            }
            if state.active.contains_key(&member.device_id) {
                return Err(SyncError::Invalid("device is already active".into()));
            }
            validate_public_key(&member.public_key)
        }
        MembershipAction::Revoke { device_id } => {
            if !is_valid_identifier(device_id, MAX_DEVICE_ID_BYTES) {
                return Err(SyncError::Invalid("device identifier is invalid".into()));
            }
            if !state.active.contains_key(device_id) {
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
    log.verify(&())?;
    if !log.active_members().contains_key(revoked_device) {
        return Err(SyncError::Invalid("device is not an active member".into()));
    }
    let mut staged = log.clone();
    staged.append_change(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::link_preimage;
    use crate::crypto::open_sealed;
    use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};
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
        log.append_change(
            MembershipAction::Add(a_member),
            "a",
            HybridLogicalClock::new("a", 1),
            &a_key,
        )
        .unwrap();
        log.append_change(
            MembershipAction::Add(b_member),
            "a",
            HybridLogicalClock::new("a", 2),
            &a_key,
        )
        .unwrap();
        (log, a_key, b_key)
    }

    fn state_before(log: &MembershipLog, index: usize) -> MembershipState {
        let mut state = MembershipState::default();
        for link in &log.entries[..index] {
            link.payload.apply(&mut state);
        }
        state
    }

    fn reseal(link: &mut MembershipLink, key: &SigningKey) {
        let preimage = link_preimage(&link.payload, &link.previous_hash).unwrap();
        link.hash = *blake3::hash(&preimage).as_bytes();
        link.signature = key.sign(&preimage).to_bytes();
    }

    #[test]
    fn membership_link_preimage_is_pinned() {
        // Hand-derived layout: domain, NUL, previous hash, action tag,
        // length-prefixed device id, both keys, length-prefixed author,
        // length-prefixed clock node, physical BE u64, logical BE u32.
        let entry = MembershipEntry {
            action: MembershipAction::Add(DeviceMember {
                device_id: "d1".into(),
                public_key: [3; 32],
                signing_key: [4; 32],
            }),
            added_by: "d1".into(),
            clock: HybridLogicalClock {
                physical_ms: 258,
                logical: 1,
                node_id: "n".into(),
            },
        };
        let mut expected = Vec::new();
        expected.extend_from_slice(b"vbuff-membership-entry-v3");
        expected.push(0);
        expected.extend_from_slice(&[7; 32]);
        expected.push(ACTION_ADD);
        expected.extend_from_slice(&2_u32.to_be_bytes());
        expected.extend_from_slice(b"d1");
        expected.extend_from_slice(&[3; 32]);
        expected.extend_from_slice(&[4; 32]);
        expected.extend_from_slice(&2_u32.to_be_bytes());
        expected.extend_from_slice(b"d1");
        expected.extend_from_slice(&1_u32.to_be_bytes());
        expected.extend_from_slice(b"n");
        expected.extend_from_slice(&258_u64.to_be_bytes());
        expected.extend_from_slice(&1_u32.to_be_bytes());
        assert_eq!(link_preimage(&entry, &[7; 32]).unwrap(), expected);

        let revoke = MembershipEntry {
            action: MembershipAction::Revoke {
                device_id: "d1".into(),
            },
            added_by: "d1".into(),
            clock: HybridLogicalClock {
                physical_ms: 258,
                logical: 1,
                node_id: "n".into(),
            },
        };
        let mut expected_revoke = Vec::new();
        expected_revoke.extend_from_slice(b"vbuff-membership-entry-v3");
        expected_revoke.push(0);
        expected_revoke.extend_from_slice(&[7; 32]);
        expected_revoke.push(ACTION_REVOKE);
        expected_revoke.extend_from_slice(&2_u32.to_be_bytes());
        expected_revoke.extend_from_slice(b"d1");
        expected_revoke.extend_from_slice(&2_u32.to_be_bytes());
        expected_revoke.extend_from_slice(b"d1");
        expected_revoke.extend_from_slice(&1_u32.to_be_bytes());
        expected_revoke.extend_from_slice(b"n");
        expected_revoke.extend_from_slice(&258_u64.to_be_bytes());
        expected_revoke.extend_from_slice(&1_u32.to_be_bytes());
        assert_eq!(link_preimage(&revoke, &[7; 32]).unwrap(), expected_revoke);
    }

    #[test]
    fn stale_v2_entries_fail_to_deserialize() {
        // The v2 shape carried the hashes next to the payload fields. A log
        // written by that build must not load at all, rather than load and
        // be re-verified under v3 rules.
        let (log, _, _) = two_device_log();
        let mut value = serde_json::to_value(&log).unwrap();
        for link in value["entries"].as_array_mut().unwrap() {
            let payload = link.as_object_mut().unwrap().remove("payload").unwrap();
            for (key, field) in payload.as_object().unwrap() {
                link[key.as_str()] = field.clone();
            }
        }
        let v2 = serde_json::to_string(&value).unwrap();
        assert!(serde_json::from_str::<MembershipLog>(&v2).is_err());
    }

    #[test]
    fn append_and_verify_share_one_expected_signing_key() {
        let (log, a_key, _) = two_device_log();
        let owner_key = a_key.verifying_key().to_bytes();
        for (index, link) in log.entries.iter().enumerate() {
            let state = state_before(&log, index);
            // The hook is the single authorization decision: `append`
            // admitted this link only because it names this key, and
            // `verify` checks the signature against the same value.
            let expected = link
                .payload
                .expected_signing_key(index, &state, &())
                .unwrap();
            assert_eq!(expected, owner_key);
            let key = VerifyingKey::from_bytes(&expected).unwrap();
            let preimage = link_preimage(&link.payload, &link.previous_hash).unwrap();
            key.verify(&preimage, &Signature::from_bytes(&link.signature))
                .unwrap();
        }
        log.verify(&()).unwrap();

        // Every other key is refused on append, by the same hook.
        let mut extended = log.clone();
        let (_, _, c_member) = member_parts("c", 45, 46);
        let next = MembershipEntry {
            action: MembershipAction::Add(c_member),
            added_by: "a".into(),
            clock: HybridLogicalClock::new("a", 3),
        };
        let state = state_before(&extended, extended.entries.len());
        assert_eq!(
            next.expected_signing_key(extended.entries.len(), &state, &())
                .unwrap(),
            owner_key
        );
        for seed in [0_u8, 1, 99] {
            let other = SigningKey::from_bytes(&[seed; 32]);
            assert_ne!(other.verifying_key().to_bytes(), owner_key);
            assert!(extended.append(next.clone(), &(), &other).is_err());
        }
        assert!(extended.append(next, &(), &a_key).is_ok());
    }

    #[test]
    fn verify_derives_the_signing_key_from_the_chain_not_from_the_link() {
        let (mut log, _, _) = two_device_log();
        let genuine_head = log.head();
        let attacker = SigningKey::from_bytes(&[77; 32]);

        // Rebind the owner's registered key and re-seal entry 0 under it.
        let MembershipAction::Add(member) = &mut log.entries[0].payload.action else {
            unreachable!()
        };
        member.signing_key = attacker.verifying_key().to_bytes();
        reseal(&mut log.entries[0], &attacker);
        log.entries[1].previous_hash = log.entries[0].hash;
        let preimage =
            link_preimage(&log.entries[1].payload, &log.entries[1].previous_hash).unwrap();
        log.entries[1].hash = *blake3::hash(&preimage).as_bytes();
        // Entry 1 still carries the real owner's signature, which no longer
        // matches the key verify derives for it from the replayed state.
        assert!(log.verify(&()).is_err());

        // Re-signing every link makes the log internally consistent again:
        // rewriting entry 0 rewrites the root of trust. It is a different
        // chain, and the head — hence the SAS — says so out of band.
        reseal(&mut log.entries[1], &attacker);
        log.verify(&()).unwrap();
        assert_ne!(log.head(), genuine_head);
    }

    #[test]
    fn signed_entries_verify_and_forged_author_is_rejected() {
        let (log, _, _) = two_device_log();
        log.verify(&()).unwrap();

        // Forged author with a recomputed hash but no matching signature.
        let mut forged_author = log.clone();
        let link = &mut forged_author.entries[1];
        link.payload.added_by = "ghost".into();
        link.hash =
            *blake3::hash(&link_preimage(&link.payload, &link.previous_hash).unwrap()).as_bytes();
        assert!(forged_author.verify(&()).is_err());

        // Forged content under the real author still fails the signature.
        let mut forged_action = log;
        let (_, _, evil) = member_parts("evil", 25, 26);
        let link = &mut forged_action.entries[1];
        link.payload.action = MembershipAction::Add(evil);
        link.hash =
            *blake3::hash(&link_preimage(&link.payload, &link.previous_hash).unwrap()).as_bytes();
        assert!(forged_action.verify(&()).is_err());
    }

    #[test]
    fn wrong_signing_key_is_rejected() {
        let wrong = SigningKey::from_bytes(&[99; 32]);

        // Entry 0: the key must match the self-declared signing key.
        let (_, _, member) = member_parts("a", 31, 32);
        let mut fresh = MembershipLog::default();
        assert!(
            fresh
                .append_change(
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
            log.append_change(
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
        log.append_change(
            MembershipAction::Revoke {
                device_id: "b".into(),
            },
            "a",
            HybridLogicalClock::new("a", 3),
            &a_key,
        )
        .unwrap();
        log.verify(&()).unwrap();

        let before = log.clone();
        let (_, _, c_member) = member_parts("c", 35, 36);
        assert!(
            log.append_change(
                MembershipAction::Add(c_member),
                "b",
                HybridLogicalClock::new("b", 4),
                &b_key,
            )
            .is_err()
        );
        assert_eq!(log, before);
        log.verify(&()).unwrap();
    }

    #[test]
    fn non_owner_active_device_cannot_author() {
        let (mut log, _, b_key) = two_device_log();
        let before = log.clone();
        let (_, _, c_member) = member_parts("c", 37, 38);
        // "b" is active and signs with its own key, but is not the owner.
        assert!(
            log.append_change(
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

        // The full twenty-digit space must be reachable. Reducing modulo
        // 10^19 while printing twenty digits would pin the first digit to
        // zero, costing 3.3 bits and wasting one of the digits the two humans
        // read aloud to each other.
        let leading: Vec<char> = (0..64_u8)
            .map(|seed| {
                log.sas(&[seed; 32], &[seed.wrapping_add(1); 32])
                    .chars()
                    .next()
                    .unwrap()
            })
            .collect();
        assert!(
            leading.iter().any(|digit| *digit != '0'),
            "the leading SAS digit never varies, so the modulus is too small"
        );

        let (_, _, c_member) = member_parts("c", 39, 40);
        log.append_change(
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
                log.append_change(
                    MembershipAction::Add(bad_member),
                    "a",
                    clock.clone(),
                    &a_key,
                )
                .is_err()
            );
            assert!(
                log.append_change(
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
                log.append_change(
                    MembershipAction::Add(c_member.clone()),
                    bad,
                    clock.clone(),
                    &a_key
                )
                .is_err()
            );
            assert!(
                log.append_change(
                    MembershipAction::Add(c_member),
                    "a",
                    HybridLogicalClock::new(bad, 10),
                    &a_key,
                )
                .is_err()
            );
        }
        assert_eq!(log.entries.len(), 2);
        log.verify(&()).unwrap();
    }

    #[test]
    fn log_is_bounded() {
        let (_, a_key, a_member) = member_parts("a", 51, 52);
        let mut log = MembershipLog::default();
        log.append_change(
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
            log.append_change(
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
        log.verify(&()).unwrap();

        let (_, _, extra) = member_parts("overflow", 53, 54);
        assert!(
            log.append_change(
                MembershipAction::Add(extra),
                "a",
                HybridLogicalClock::new("a", MAX_MEMBERSHIP_ENTRIES as u64 + 1),
                &a_key,
            )
            .is_err()
        );

        // A log constructed past the bound fails closed in verify.
        log.entries.push(log.entries[0].clone());
        assert!(log.verify(&()).is_err());
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
        log.append_change(
            MembershipAction::Add(a_member),
            "a",
            HybridLogicalClock::new("a", 1),
            &a_key,
        )
        .unwrap();
        log.verify(&()).unwrap();
        assert_eq!(log.sas(&[2; 32], &[3; 32]), log.sas(&[3; 32], &[2; 32]));
        log.entries[0].payload.added_by = "attacker".into();
        assert!(log.verify(&()).is_err());
    }

    #[test]
    fn verification_replays_authorization_even_if_hashes_are_recomputed() {
        let (mut log, _, _) = two_device_log();
        let link = &mut log.entries[1];
        link.payload.added_by = "ghost".into();
        link.hash =
            *blake3::hash(&link_preimage(&link.payload, &link.previous_hash).unwrap()).as_bytes();
        assert!(log.verify(&()).is_err());
    }

    #[test]
    fn revoked_device_receives_no_new_epoch_key() {
        let (a_secret, a_key, a_member) = member_parts("a", 11, 61);
        let (_, _, b_member) = member_parts("b", 12, 62);
        let mut log = MembershipLog::default();
        log.append_change(
            MembershipAction::Add(a_member),
            "a",
            HybridLogicalClock::new("a", 1),
            &a_key,
        )
        .unwrap();
        log.append_change(
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
            log.append_change(
                MembershipAction::Add(low_order),
                "a",
                HybridLogicalClock::new("a", 1),
                &a_key,
            )
            .is_err()
        );

        let (_, _, member) = member_parts("a", 10, 60);
        log.append_change(
            MembershipAction::Add(member),
            "a",
            HybridLogicalClock::new("a", 1),
            &a_key,
        )
        .unwrap();
        assert!(
            log.append_change(
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
        log.append_change(
            MembershipAction::Add(a_member),
            "a",
            HybridLogicalClock::new("a", 1),
            &a_key,
        )
        .unwrap();
        log.append_change(
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
