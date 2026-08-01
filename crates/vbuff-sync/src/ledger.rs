//! Signed wipe receipts and a tamper-evident local sync audit ledger.
//!
//! The ledger is a [`SignedChain`]; the chain mechanics live in
//! [`crate::chain`] and this module supplies only the payload layout and the
//! authorization rule.

use std::collections::BTreeMap;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use vbuff_types::validation::is_valid_identifier;

use crate::chain::{ChainEntry, ChainLink, Preimage, SignedChain};
use crate::clock::HybridLogicalClock;
use crate::{Result, SyncError};

/// Maximum byte length of device identifiers in a ledger entry.
const MAX_DEVICE_ID_BYTES: usize = 128;
/// Fail-closed bound on the number of ledger entries.
///
/// The ledger is an in-memory audit trail; callers checkpoint and start a
/// fresh chain rather than growing without limit. An unbounded chain is a
/// verification-cost and memory amplifier for anything that can drive sync
/// events, so the bound is enforced on write and on read alike.
const MAX_LEDGER_ENTRIES: usize = 16_384;
/// Domain for the wipe-receipt signature preimage.
const WIPE_RECEIPT_DOMAIN: &[u8] = b"vbuff-wipe-receipt-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncDirection {
    Sent,
    Received,
}

impl SyncDirection {
    /// Stable preimage discriminant, independent of any serde rename.
    const fn discriminant(self) -> u8 {
        match self {
            Self::Sent => 1,
            Self::Received => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncLedgerDecision {
    Allowed,
    DeniedByPolicy,
    Applied,
    RejectedEpoch,
}

impl SyncLedgerDecision {
    /// Stable preimage discriminant, independent of any serde rename.
    const fn discriminant(self) -> u8 {
        match self {
            Self::Allowed => 1,
            Self::DeniedByPolicy => 2,
            Self::Applied => 3,
            Self::RejectedEpoch => 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncEvent {
    pub item_hash: [u8; 32],
    pub peer_device: String,
    pub direction: SyncDirection,
    pub epoch: u64,
    pub decision: SyncLedgerDecision,
    pub clock: HybridLogicalClock,
}

/// The signed payload of one ledger link.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerEntry {
    pub signer_device: String,
    pub event: SyncEvent,
}

/// Device signing keys trusted to author ledger entries.
pub type TrustedLedgerKeys = BTreeMap<String, [u8; 32]>;

/// Local sync audit ledger: a [`SignedChain`] of [`LedgerEntry`] payloads.
pub type SyncLedger = SignedChain<LedgerEntry>;
/// One link of a [`SyncLedger`].
pub type SignedLedgerEntry = ChainLink<LedgerEntry>;

impl ChainEntry for LedgerEntry {
    const DOMAIN: &'static [u8] = b"vbuff-sync-ledger-v2";
    const MAX_ENTRIES: usize = MAX_LEDGER_ENTRIES;
    const LABEL: &'static str = "sync ledger";

    type Authority = TrustedLedgerKeys;
    type State = ();

    fn extend_preimage(&self, preimage: &mut Preimage) {
        preimage
            .var(self.signer_device.as_bytes())
            .fixed(&self.event.item_hash)
            .var(self.event.peer_device.as_bytes())
            .byte(self.event.direction.discriminant())
            .u64_be(self.event.epoch)
            .byte(self.event.decision.discriminant())
            .var(self.event.clock.node_id.as_bytes())
            .u64_be(self.event.clock.physical_ms)
            .u32_be(self.event.clock.logical);
    }

    /// The one key permitted to sign this entry.
    ///
    /// Enforced identically on append and on verify, so an entry can never
    /// be written by a device the ledger would later refuse to attribute.
    fn expected_signing_key(
        &self,
        _index: usize,
        _state: &(),
        keys: &TrustedLedgerKeys,
    ) -> Result<[u8; 32]> {
        for identifier in [
            &self.signer_device,
            &self.event.peer_device,
            &self.event.clock.node_id,
        ] {
            if !is_valid_identifier(identifier, MAX_DEVICE_ID_BYTES) {
                return Err(SyncError::Invalid(
                    "sync ledger device identifier is invalid".into(),
                ));
            }
        }
        keys.get(&self.signer_device)
            .copied()
            .ok_or_else(|| SyncError::Invalid("unknown ledger signer".into()))
    }
}

impl SyncLedger {
    /// Record `event` as observed and signed by `signer_device`.
    pub fn append_event(
        &mut self,
        signer_device: impl Into<String>,
        event: SyncEvent,
        keys: &TrustedLedgerKeys,
        signing_key: &SigningKey,
    ) -> Result<[u8; 32]> {
        self.append(
            LedgerEntry {
                signer_device: signer_device.into(),
                event,
            },
            keys,
            signing_key,
        )
    }
}

/// Wire schema of [`WipeReceipt`].
///
/// Receipts issued before the signature was domain-separated carry no
/// `schema` field, so they now fail to *deserialize* rather than
/// deserializing and then failing verification. That distinction matters:
/// without it a stale receipt is reported as a forged one, and an operator
/// looking at a genuine format break sees a tampering alert.
const WIPE_RECEIPT_SCHEMA: u16 = 2;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WipeReceipt {
    /// Always [`WIPE_RECEIPT_SCHEMA`]; validated before the signature is
    /// checked and covered by the signature so it cannot be rewritten.
    pub schema: u16,
    pub device_id: String,
    pub item_hash: [u8; 32],
    pub epoch: u64,
    pub applied_at_ms: u64,
    #[serde(with = "serde_receipt_signature")]
    pub signature: [u8; 64],
}

/// Serde support for the fixed-width receipt signature; deserialization
/// fails closed on any other length.
mod serde_receipt_signature {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(value: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error> {
        value.as_slice().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 64], D::Error> {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        <[u8; 64]>::try_from(bytes.as_slice())
            .map_err(|_| serde::de::Error::custom("wipe receipt signature must be 64 bytes"))
    }
}

pub fn issue_wipe_receipt(
    device_id: impl Into<String>,
    item_hash: [u8; 32],
    epoch: u64,
    applied_at_ms: u64,
    signing_key: &SigningKey,
) -> Result<WipeReceipt> {
    let mut receipt = WipeReceipt {
        schema: WIPE_RECEIPT_SCHEMA,
        device_id: device_id.into(),
        item_hash,
        epoch,
        applied_at_ms,
        signature: [0; 64],
    };
    if !is_valid_identifier(&receipt.device_id, MAX_DEVICE_ID_BYTES) {
        return Err(SyncError::Invalid(
            "wipe receipt device identifier is invalid".into(),
        ));
    }
    receipt.signature = signing_key.sign(&receipt_payload(&receipt)?).to_bytes();
    Ok(receipt)
}

pub fn verify_wipe_receipt(receipt: &WipeReceipt, key: &VerifyingKey) -> Result<()> {
    // Checked before the signature so an unsupported receipt is reported as
    // unsupported rather than as a bad signature.
    if receipt.schema != WIPE_RECEIPT_SCHEMA {
        return Err(SyncError::Invalid(format!(
            "wipe receipt schema {} is unsupported",
            receipt.schema
        )));
    }
    key.verify(
        &receipt_payload(receipt)?,
        &Signature::from_bytes(&receipt.signature),
    )
    .map_err(|_| SyncError::Crypto)
}

fn receipt_payload(receipt: &WipeReceipt) -> Result<Vec<u8>> {
    let mut preimage = Preimage::new(WIPE_RECEIPT_DOMAIN);
    preimage
        .u64_be(u64::from(receipt.schema))
        .var(receipt.device_id.as_bytes())
        .fixed(&receipt.item_hash)
        .u64_be(receipt.epoch)
        .u64_be(receipt.applied_at_ms);
    preimage.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::link_preimage;

    fn event() -> SyncEvent {
        SyncEvent {
            item_hash: [4; 32],
            peer_device: "phone".into(),
            direction: SyncDirection::Sent,
            epoch: 2,
            decision: SyncLedgerDecision::Allowed,
            clock: HybridLogicalClock::new("laptop", 10),
        }
    }

    fn entry(signer: &str) -> LedgerEntry {
        LedgerEntry {
            signer_device: signer.into(),
            event: event(),
        }
    }

    fn laptop_keys(key: &SigningKey) -> TrustedLedgerKeys {
        TrustedLedgerKeys::from([("laptop".into(), key.verifying_key().to_bytes())])
    }

    #[test]
    fn ledger_link_preimage_is_pinned() {
        // Hand-derived layout: domain, NUL, previous hash, length-prefixed
        // signer, item hash, length-prefixed peer, direction tag, epoch BE
        // u64, decision tag, length-prefixed clock node, physical BE u64,
        // logical BE u32.
        let mut expected = Vec::new();
        expected.extend_from_slice(b"vbuff-sync-ledger-v2");
        expected.push(0);
        expected.extend_from_slice(&[7; 32]);
        expected.extend_from_slice(&6_u32.to_be_bytes());
        expected.extend_from_slice(b"laptop");
        expected.extend_from_slice(&[4; 32]);
        expected.extend_from_slice(&5_u32.to_be_bytes());
        expected.extend_from_slice(b"phone");
        expected.push(1);
        expected.extend_from_slice(&2_u64.to_be_bytes());
        expected.push(1);
        expected.extend_from_slice(&6_u32.to_be_bytes());
        expected.extend_from_slice(b"laptop");
        expected.extend_from_slice(&10_u64.to_be_bytes());
        expected.extend_from_slice(&0_u32.to_be_bytes());
        assert_eq!(link_preimage(&entry("laptop"), &[7; 32]).unwrap(), expected);
    }

    #[test]
    fn stale_v1_entries_fail_to_deserialize() {
        // v1 stored `signer_device` and `event` next to the hashes.
        let key = SigningKey::from_bytes(&[7; 32]);
        let mut ledger = SyncLedger::default();
        ledger
            .append(entry("laptop"), &laptop_keys(&key), &key)
            .unwrap();
        let mut value = serde_json::to_value(&ledger).unwrap();
        for link in value["entries"].as_array_mut().unwrap() {
            let payload = link.as_object_mut().unwrap().remove("payload").unwrap();
            for (field, content) in payload.as_object().unwrap() {
                link[field.as_str()] = content.clone();
            }
        }
        let v1 = serde_json::to_string(&value).unwrap();
        assert!(serde_json::from_str::<SyncLedger>(&v1).is_err());
    }

    #[test]
    fn signed_ledger_detects_tampering() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let keys = laptop_keys(&key);
        let mut ledger = SyncLedger::default();
        ledger.append_event("laptop", event(), &keys, &key).unwrap();
        ledger.verify(&keys).unwrap();
        ledger.entries[0].payload.event.epoch = 99;
        assert!(ledger.verify(&keys).is_err());
    }

    #[test]
    fn forged_author_with_a_recomputed_hash_is_rejected() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let ghost = SigningKey::from_bytes(&[8; 32]);
        let mut keys = laptop_keys(&key);
        keys.insert("ghost".into(), ghost.verifying_key().to_bytes());
        let mut ledger = SyncLedger::default();
        ledger.append(entry("laptop"), &keys, &key).unwrap();

        let link = &mut ledger.entries[0];
        link.payload.signer_device = "ghost".into();
        let preimage = link_preimage(&link.payload, &link.previous_hash).unwrap();
        link.hash = *blake3::hash(&preimage).as_bytes();
        // The chain is internally consistent again, but the signature was
        // made by "laptop" and the hook now names the ghost's key.
        assert!(ledger.verify(&keys).is_err());
    }

    #[test]
    fn an_unregistered_signer_cannot_append_and_the_ledger_is_untouched() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let keys = laptop_keys(&key);
        let mut ledger = SyncLedger::default();
        ledger.append(entry("laptop"), &keys, &key).unwrap();
        let before = ledger.clone();

        let stranger = SigningKey::from_bytes(&[9; 32]);
        assert!(ledger.append(entry("desktop"), &keys, &stranger).is_err());
        // A registered name signed with the wrong key is refused too.
        assert!(ledger.append(entry("laptop"), &keys, &stranger).is_err());
        assert_eq!(ledger, before);
        ledger.verify(&keys).unwrap();

        // A revoked device: dropping the key rejects the whole history.
        assert!(ledger.verify(&TrustedLedgerKeys::new()).is_err());
    }

    #[test]
    fn identifiers_are_validated() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let mut keys = laptop_keys(&key);
        let mut ledger = SyncLedger::default();
        let overlong = "x".repeat(MAX_DEVICE_ID_BYTES + 1);
        for bad in ["", "has space", "устройство", overlong.as_str()] {
            let mut payload = entry("laptop");
            payload.event.peer_device = bad.into();
            assert!(ledger.append(payload, &keys, &key).is_err());

            let mut payload = entry("laptop");
            payload.event.clock = HybridLogicalClock::new(bad, 10);
            assert!(ledger.append(payload, &keys, &key).is_err());

            keys.insert(bad.into(), key.verifying_key().to_bytes());
            assert!(ledger.append(entry(bad), &keys, &key).is_err());
        }
        assert!(ledger.is_empty());
    }

    #[test]
    fn ledger_is_bounded() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let keys = laptop_keys(&key);
        let mut ledger = SyncLedger::default();
        ledger.append(entry("laptop"), &keys, &key).unwrap();

        // Filling the chain by hand keeps the test cheap; the bound is
        // checked before anything else, so the links need not be valid.
        let link = ledger.entries[0].clone();
        ledger.entries.resize(MAX_LEDGER_ENTRIES, link.clone());
        assert_eq!(ledger.len(), MAX_LEDGER_ENTRIES);
        let full = ledger.append(entry("laptop"), &keys, &key).unwrap_err();
        assert!(full.to_string().contains("sync ledger is full"), "{full}");
        assert_eq!(ledger.len(), MAX_LEDGER_ENTRIES);

        // One past the bound fails closed on read as well, before any link
        // is examined.
        ledger.entries.push(link);
        let over = ledger.verify(&keys).unwrap_err();
        assert!(
            over.to_string().contains("exceeds the entry limit"),
            "{over}"
        );
    }

    #[test]
    fn wipe_receipt_is_bound_to_item_device_and_epoch() {
        let key = SigningKey::from_bytes(&[8; 32]);
        let mut receipt = issue_wipe_receipt("phone", [3; 32], 4, 100, &key).unwrap();
        verify_wipe_receipt(&receipt, &key.verifying_key()).unwrap();
        receipt.epoch = 5;
        assert!(verify_wipe_receipt(&receipt, &key.verifying_key()).is_err());
        assert!(issue_wipe_receipt("has space", [3; 32], 4, 100, &key).is_err());
    }

    #[test]
    fn wipe_receipt_preimage_is_pinned() {
        let receipt = WipeReceipt {
            schema: WIPE_RECEIPT_SCHEMA,
            device_id: "phone".into(),
            item_hash: [3; 32],
            epoch: 4,
            applied_at_ms: 100,
            signature: [0; 64],
        };
        let mut expected = Vec::new();
        expected.extend_from_slice(b"vbuff-wipe-receipt-v1");
        expected.push(0);
        expected.extend_from_slice(&u64::from(WIPE_RECEIPT_SCHEMA).to_be_bytes());
        expected.extend_from_slice(&5_u32.to_be_bytes());
        expected.extend_from_slice(b"phone");
        expected.extend_from_slice(&[3; 32]);
        expected.extend_from_slice(&4_u64.to_be_bytes());
        expected.extend_from_slice(&100_u64.to_be_bytes());
        assert_eq!(receipt_payload(&receipt).unwrap(), expected);
    }

    #[test]
    fn a_receipt_from_before_domain_separation_reads_as_stale_not_as_forged() {
        // The pre-v2 wire shape carried no `schema`. It must fail while being
        // read, so a genuine format break is never reported as tampering.
        let stale = serde_json::json!({
            "device_id": "phone",
            "item_hash": vec![3_u8; 32],
            "epoch": 4,
            "applied_at_ms": 100,
            "signature": vec![0_u8; 64],
        });
        let error = serde_json::from_value::<WipeReceipt>(stale).unwrap_err();
        assert!(error.to_string().contains("schema"), "{error}");
    }

    #[test]
    fn an_unsupported_schema_is_refused_before_the_signature_is_judged() {
        let key = SigningKey::from_bytes(&[8; 32]);
        let mut receipt = issue_wipe_receipt("phone", [3; 32], 4, 100, &key).unwrap();
        receipt.schema = WIPE_RECEIPT_SCHEMA + 1;
        let error = verify_wipe_receipt(&receipt, &key.verifying_key()).unwrap_err();
        assert!(
            matches!(error, SyncError::Invalid(ref message) if message.contains("unsupported")),
            "{error:?}"
        );
    }
}
