//! Hash-chained device membership, whole-set SAS, and epoch revocation.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::clock::HybridLogicalClock;
use crate::crypto::{SealedEnvelope, seal_to};
use crate::{Result, SyncError};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceMember {
    pub device_id: String,
    pub public_key: [u8; 32],
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
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembershipLog {
    pub entries: Vec<MembershipEntry>,
}

impl MembershipLog {
    pub fn append(
        &mut self,
        action: MembershipAction,
        added_by: impl Into<String>,
        clock: HybridLogicalClock,
    ) -> Result<[u8; 32]> {
        let added_by = added_by.into();
        let active = self.active_members();
        validate_entry_semantics(
            self.entries.is_empty(),
            &active,
            &action,
            &added_by,
            &clock,
            self.entries.last().map(|entry| &entry.clock),
        )?;
        let previous_hash = self.head();
        let hash = entry_hash(&action, &added_by, &clock, &previous_hash)?;
        self.entries.push(MembershipEntry {
            action,
            added_by,
            clock,
            previous_hash,
            hash,
        });
        Ok(hash)
    }

    pub fn head(&self) -> [u8; 32] {
        self.entries.last().map_or([0; 32], |entry| entry.hash)
    }

    pub fn verify(&self) -> Result<()> {
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
                &active,
                &entry.action,
                &entry.added_by,
                &entry.clock,
                previous_clock,
            )?;
            apply_action(&mut active, &entry.action);
            previous = entry.hash;
            previous_clock = Some(&entry.clock);
        }
        Ok(())
    }

    pub fn active_members(&self) -> BTreeMap<String, [u8; 32]> {
        let mut members = BTreeMap::new();
        for entry in &self.entries {
            match &entry.action {
                MembershipAction::Add(member) => {
                    members.insert(member.device_id.clone(), member.public_key);
                }
                MembershipAction::Revoke { device_id } => {
                    members.remove(device_id);
                }
            }
        }
        members
    }

    /// Six-digit SAS commits to the whole membership head and both pairing keys.
    pub fn sas(&self, first_key: &[u8; 32], second_key: &[u8; 32]) -> String {
        let (left, right) = if first_key <= second_key {
            (first_key, second_key)
        } else {
            (second_key, first_key)
        };
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"vbuff-membership-sas-v1");
        hasher.update(&self.head());
        hasher.update(left);
        hasher.update(right);
        let bytes = hasher.finalize();
        let value = u32::from_le_bytes(bytes.as_bytes()[0..4].try_into().unwrap()) % 1_000_000;
        format!("{value:06}")
    }
}

fn validate_entry_semantics(
    first: bool,
    active: &BTreeMap<String, [u8; 32]>,
    action: &MembershipAction,
    added_by: &str,
    clock: &HybridLogicalClock,
    previous_clock: Option<&HybridLogicalClock>,
) -> Result<()> {
    if added_by.is_empty() {
        return Err(SyncError::Invalid("membership author is empty".into()));
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
    } else if !active.contains_key(added_by) {
        return Err(SyncError::Invalid(
            "membership change was not authorized by an active device".into(),
        ));
    }
    if previous_clock.is_some_and(|previous| clock <= previous) {
        return Err(SyncError::Invalid(
            "membership clock must advance monotonically".into(),
        ));
    }
    match action {
        MembershipAction::Add(member) => {
            if member.device_id.is_empty() {
                return Err(SyncError::Invalid("device ID is empty".into()));
            }
            if active.contains_key(&member.device_id) {
                return Err(SyncError::Invalid("device is already active".into()));
            }
            validate_public_key(&member.public_key)
        }
        MembershipAction::Revoke { device_id } => {
            if !active.contains_key(device_id) {
                return Err(SyncError::Invalid(
                    "cannot revoke a device that is not active".into(),
                ));
            }
            Ok(())
        }
    }
}

fn apply_action(active: &mut BTreeMap<String, [u8; 32]>, action: &MembershipAction) {
    match action {
        MembershipAction::Add(member) => {
            active.insert(member.device_id.clone(), member.public_key);
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

pub fn revoke_and_rekey(
    log: &mut MembershipLog,
    revoked_device: &str,
    added_by: &str,
    clock: HybridLogicalClock,
    current_epoch: u64,
    new_group_key: &[u8; 32],
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
    )?;
    let epoch = current_epoch
        .checked_add(1)
        .ok_or_else(|| SyncError::Invalid("membership epoch exhausted".into()))?;
    let aad = format!("vbuff-group-epoch-{epoch}");
    let wrapped_group_keys = staged
        .active_members()
        .into_iter()
        .map(|(device, public_key)| {
            Ok((device, seal_to(&public_key, new_group_key, aad.as_bytes())?))
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
    hasher.update(b"vbuff-membership-entry-v1");
    hasher.update(previous_hash);
    hasher.update(&serde_json::to_vec(&(action, added_by, clock))?);
    Ok(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::open_sealed;
    use x25519_dalek::{PublicKey, StaticSecret};

    #[test]
    fn sas_commits_to_full_verified_chain() {
        let mut log = MembershipLog::default();
        let secret = StaticSecret::from([9; 32]);
        log.append(
            MembershipAction::Add(DeviceMember {
                device_id: "a".into(),
                public_key: PublicKey::from(&secret).to_bytes(),
            }),
            "a",
            HybridLogicalClock::new("a", 1),
        )
        .unwrap();
        log.verify().unwrap();
        assert_eq!(log.sas(&[2; 32], &[3; 32]), log.sas(&[3; 32], &[2; 32]));
        log.entries[0].added_by = "attacker".into();
        assert!(log.verify().is_err());
    }

    #[test]
    fn verification_replays_authorization_even_if_hashes_are_recomputed() {
        let a_secret = StaticSecret::from([31; 32]);
        let b_secret = StaticSecret::from([32; 32]);
        let mut log = MembershipLog::default();
        log.append(
            MembershipAction::Add(DeviceMember {
                device_id: "a".into(),
                public_key: PublicKey::from(&a_secret).to_bytes(),
            }),
            "a",
            HybridLogicalClock::new("a", 1),
        )
        .unwrap();
        log.append(
            MembershipAction::Add(DeviceMember {
                device_id: "b".into(),
                public_key: PublicKey::from(&b_secret).to_bytes(),
            }),
            "a",
            HybridLogicalClock::new("a", 2),
        )
        .unwrap();

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
        let a_secret = StaticSecret::from([11; 32]);
        let b_secret = StaticSecret::from([12; 32]);
        let mut log = MembershipLog::default();
        for (counter, (id, secret)) in [("a", &a_secret), ("b", &b_secret)].into_iter().enumerate()
        {
            log.append(
                MembershipAction::Add(DeviceMember {
                    device_id: id.into(),
                    public_key: PublicKey::from(secret).to_bytes(),
                }),
                "a",
                HybridLogicalClock::new("a", counter as u64 + 1),
            )
            .unwrap();
        }
        let transition = revoke_and_rekey(
            &mut log,
            "b",
            "a",
            HybridLogicalClock::new("a", 3),
            4,
            &[99; 32],
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
        let mut log = MembershipLog::default();
        assert!(
            log.append(
                MembershipAction::Add(DeviceMember {
                    device_id: "a".into(),
                    public_key: [0; 32],
                }),
                "a",
                HybridLogicalClock::new("a", 1),
            )
            .is_err()
        );

        let secret = StaticSecret::from([10; 32]);
        log.append(
            MembershipAction::Add(DeviceMember {
                device_id: "a".into(),
                public_key: PublicKey::from(&secret).to_bytes(),
            }),
            "a",
            HybridLogicalClock::new("a", 1),
        )
        .unwrap();
        assert!(
            log.append(
                MembershipAction::Revoke {
                    device_id: "a".into(),
                },
                "attacker",
                HybridLogicalClock::new("attacker", 2),
            )
            .is_err()
        );
    }

    #[test]
    fn failed_epoch_transition_does_not_publish_the_revocation() {
        let a_secret = StaticSecret::from([41; 32]);
        let b_secret = StaticSecret::from([42; 32]);
        let mut log = MembershipLog::default();
        for (counter, (id, secret)) in [("a", &a_secret), ("b", &b_secret)].into_iter().enumerate()
        {
            log.append(
                MembershipAction::Add(DeviceMember {
                    device_id: id.into(),
                    public_key: PublicKey::from(secret).to_bytes(),
                }),
                "a",
                HybridLogicalClock::new("a", counter as u64 + 1),
            )
            .unwrap();
        }
        let before = log.clone();

        assert!(
            revoke_and_rekey(
                &mut log,
                "b",
                "a",
                HybridLogicalClock::new("a", 3),
                u64::MAX,
                &[7; 32],
            )
            .is_err()
        );
        assert_eq!(log, before);
    }
}
