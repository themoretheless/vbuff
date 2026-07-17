//! Single-use secret delivery and verified wipe state machine.

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::VerifyingKey;

use crate::ledger::{WipeReceipt, verify_wipe_receipt};
use crate::{Result, SyncError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BurnState {
    Armed,
    Delivered { target_device: String },
    Pasted { target_device: String },
    Wiping,
    Complete,
}

#[derive(Clone, Debug)]
pub struct BurnSession {
    item_hash: [u8; 32],
    epoch: u64,
    expected_devices: BTreeSet<String>,
    wiped_devices: BTreeSet<String>,
    pub state: BurnState,
}

impl BurnSession {
    pub fn new(
        item_hash: [u8; 32],
        epoch: u64,
        expected_devices: BTreeSet<String>,
    ) -> Result<Self> {
        if epoch == 0
            || expected_devices.is_empty()
            || expected_devices.iter().any(String::is_empty)
        {
            return Err(SyncError::Invalid("invalid burn session".into()));
        }
        Ok(Self {
            item_hash,
            epoch,
            expected_devices,
            wiped_devices: BTreeSet::new(),
            state: BurnState::Armed,
        })
    }

    pub fn delivered(&mut self, target_device: impl Into<String>) -> Result<()> {
        if self.state != BurnState::Armed {
            return Err(SyncError::Invalid("burn clip was already delivered".into()));
        }
        let target_device = target_device.into();
        if !self.expected_devices.contains(&target_device) {
            return Err(SyncError::Invalid("unexpected burn target".into()));
        }
        self.state = BurnState::Delivered { target_device };
        Ok(())
    }

    pub fn pasted_once(&mut self) -> Result<()> {
        let BurnState::Delivered { target_device } = &self.state else {
            return Err(SyncError::Invalid(
                "burn clip cannot be pasted in this state".into(),
            ));
        };
        self.state = BurnState::Pasted {
            target_device: target_device.clone(),
        };
        Ok(())
    }

    pub fn begin_wipe(&mut self) -> Result<()> {
        if !matches!(self.state, BurnState::Pasted { .. }) {
            return Err(SyncError::Invalid("burn clip was not pasted".into()));
        }
        self.state = BurnState::Wiping;
        Ok(())
    }

    pub fn accept_receipt(
        &mut self,
        receipt: &WipeReceipt,
        keys: &BTreeMap<String, VerifyingKey>,
    ) -> Result<bool> {
        if self.state != BurnState::Wiping
            || receipt.item_hash != self.item_hash
            || receipt.epoch != self.epoch
            || !self.expected_devices.contains(&receipt.device_id)
        {
            return Err(SyncError::Invalid(
                "wipe receipt does not match burn session".into(),
            ));
        }
        let key = keys
            .get(&receipt.device_id)
            .ok_or_else(|| SyncError::Invalid("unknown wipe signer".into()))?;
        verify_wipe_receipt(receipt, key)?;
        self.wiped_devices.insert(receipt.device_id.clone());
        if self.wiped_devices == self.expected_devices {
            self.state = BurnState::Complete;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use crate::ledger::issue_wipe_receipt;

    use super::*;

    #[test]
    fn burn_allows_one_paste_and_requires_every_signed_wipe() {
        let laptop = SigningKey::from_bytes(&[1; 32]);
        let phone = SigningKey::from_bytes(&[2; 32]);
        let item_hash = [9; 32];
        let mut session = BurnSession::new(
            item_hash,
            4,
            BTreeSet::from(["laptop".into(), "phone".into()]),
        )
        .unwrap();
        session.delivered("phone").unwrap();
        session.pasted_once().unwrap();
        assert!(session.pasted_once().is_err());
        session.begin_wipe().unwrap();
        let keys = BTreeMap::from([
            ("laptop".into(), laptop.verifying_key()),
            ("phone".into(), phone.verifying_key()),
        ]);
        let first = issue_wipe_receipt("phone", item_hash, 4, 10, &phone).unwrap();
        assert!(!session.accept_receipt(&first, &keys).unwrap());
        let second = issue_wipe_receipt("laptop", item_hash, 4, 11, &laptop).unwrap();
        assert!(session.accept_receipt(&second, &keys).unwrap());
        assert_eq!(session.state, BurnState::Complete);
    }
}
