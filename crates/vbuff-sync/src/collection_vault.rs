//! Per-collection key domains and explicit vault lock state.

use std::collections::BTreeMap;

use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::{Result, SyncError};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CollectionVaultId(String);

impl CollectionVaultId {
    pub fn new(id: impl Into<String>) -> Result<Self> {
        let id = id.into();
        let valid = !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
        if !valid {
            return Err(SyncError::Invalid("invalid collection vault id".into()));
        }
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultLockState {
    #[default]
    Locked,
    Unlocked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionVaultPolicy {
    pub sync_allowed: bool,
    pub user_presence_required: bool,
}

impl Default for CollectionVaultPolicy {
    fn default() -> Self {
        Self {
            sync_allowed: false,
            user_presence_required: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionVaultState {
    pub id: CollectionVaultId,
    pub key_epoch: u64,
    pub lock_state: VaultLockState,
    pub policy: CollectionVaultPolicy,
}

impl CollectionVaultState {
    pub fn permits_read(&self) -> bool {
        self.lock_state == VaultLockState::Unlocked
    }

    pub fn permits_sync(&self) -> bool {
        self.permits_read() && self.policy.sync_allowed
    }
}

#[derive(Clone, Debug, Default)]
pub struct CollectionVaultRegistry {
    vaults: BTreeMap<CollectionVaultId, CollectionVaultState>,
}

impl CollectionVaultRegistry {
    pub fn register(&mut self, state: CollectionVaultState) -> Result<()> {
        if self.vaults.contains_key(&state.id) {
            return Err(SyncError::Invalid("collection vault already exists".into()));
        }
        self.vaults.insert(state.id.clone(), state);
        Ok(())
    }

    pub fn set_lock_state(&mut self, id: &CollectionVaultId, state: VaultLockState) -> Result<()> {
        let vault = self
            .vaults
            .get_mut(id)
            .ok_or_else(|| SyncError::Invalid("unknown collection vault".into()))?;
        vault.lock_state = state;
        Ok(())
    }

    pub fn get(&self, id: &CollectionVaultId) -> Option<&CollectionVaultState> {
        self.vaults.get(id)
    }
}

pub fn derive_collection_key(
    root_key: &[u8; 32],
    id: &CollectionVaultId,
    epoch: u64,
) -> Result<Zeroizing<[u8; 32]>> {
    let hkdf = Hkdf::<Sha256>::new(Some(b"vbuff-collection-vault-v1"), root_key);
    let mut info = Vec::with_capacity(id.as_str().len() + 8);
    info.extend_from_slice(id.as_str().as_bytes());
    info.extend_from_slice(&epoch.to_be_bytes());
    let mut key = Zeroizing::new([0_u8; 32]);
    hkdf.expand(&info, &mut *key)
        .map_err(|_| SyncError::Crypto)?;
    Ok(key)
}

pub fn derive_isolated_collection_key(
    root_key: &[u8; 32],
    unlock_secret: &[u8],
    id: &CollectionVaultId,
    epoch: u64,
) -> Result<Zeroizing<[u8; 32]>> {
    if unlock_secret.len() < 16 {
        return Err(SyncError::Invalid(
            "collection unlock secret is too short".into(),
        ));
    }
    let mut input = Zeroizing::new(Vec::with_capacity(32 + unlock_secret.len()));
    input.extend_from_slice(root_key);
    input.extend_from_slice(unlock_secret);
    let hkdf = Hkdf::<Sha256>::new(Some(b"vbuff-isolated-collection-v1"), &input);
    let mut info = Vec::with_capacity(id.as_str().len() + 8);
    info.extend_from_slice(id.as_str().as_bytes());
    info.extend_from_slice(&epoch.to_be_bytes());
    let mut key = Zeroizing::new([0_u8; 32]);
    hkdf.expand(&info, &mut *key)
        .map_err(|_| SyncError::Crypto)?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_and_epoch_produce_isolated_keys() {
        let root = [7; 32];
        let work = CollectionVaultId::new("work").unwrap();
        let private = CollectionVaultId::new("private").unwrap();
        assert_ne!(
            *derive_collection_key(&root, &work, 1).unwrap(),
            *derive_collection_key(&root, &private, 1).unwrap()
        );
        assert_ne!(
            *derive_collection_key(&root, &work, 1).unwrap(),
            *derive_collection_key(&root, &work, 2).unwrap()
        );
    }

    #[test]
    fn locked_vault_never_permits_read_or_sync() {
        let state = CollectionVaultState {
            id: CollectionVaultId::new("private").unwrap(),
            key_epoch: 1,
            lock_state: VaultLockState::Locked,
            policy: CollectionVaultPolicy {
                sync_allowed: true,
                user_presence_required: true,
            },
        };
        assert!(!state.permits_read());
        assert!(!state.permits_sync());
    }

    #[test]
    fn isolated_vault_requires_a_distinct_unlock_secret() {
        let id = CollectionVaultId::new("private").unwrap();
        let first =
            derive_isolated_collection_key(&[1; 32], b"correct horse battery", &id, 1).unwrap();
        let second =
            derive_isolated_collection_key(&[1; 32], b"different unlock key", &id, 1).unwrap();
        assert_ne!(*first, *second);
        assert!(derive_isolated_collection_key(&[1; 32], b"short", &id, 1).is_err());
    }
}
