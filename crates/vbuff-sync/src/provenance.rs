//! Signed clip chain-of-custody records.
//!
//! The chain is a [`SignedChain`]; the chain mechanics live in
//! [`crate::chain`] and this module supplies only the payload layout and the
//! authorization rule.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use vbuff_types::validation::{is_valid_identifier, is_valid_label};

use crate::chain::{ChainEntry, ChainLink, Preimage, SignedChain};
use crate::{Result, SyncError};

/// Maximum byte length of device identifiers in a custody event.
const MAX_DEVICE_ID_BYTES: usize = 128;
/// Maximum byte length of the source application label.
const MAX_SOURCE_APP_BYTES: usize = 512;
/// Fail-closed bound on the number of custody entries per chain.
const MAX_CUSTODY_ENTRIES: usize = 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustodyAction {
    Captured,
    Sent,
    Received,
    Pasted,
    Burned,
}

impl CustodyAction {
    /// Stable preimage discriminant, independent of any serde rename.
    const fn discriminant(self) -> u8 {
        match self {
            Self::Captured => 1,
            Self::Sent => 2,
            Self::Received => 3,
            Self::Pasted => 4,
            Self::Burned => 5,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustodyEvent {
    pub item_hash: [u8; 32],
    pub action: CustodyAction,
    pub device_id: String,
    pub peer_device: Option<String>,
    pub source_app: Option<String>,
    pub timestamp_ms: u64,
    pub sensitive: bool,
}

impl std::fmt::Debug for CustodyEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CustodyEvent")
            .field("action", &self.action)
            .field("device_id", &self.device_id)
            .field("peer_device", &self.peer_device)
            .field(
                "source_app",
                &self.source_app.as_ref().map(|_| "[redacted]"),
            )
            .field("timestamp_ms", &self.timestamp_ms)
            .field("sensitive", &self.sensitive)
            .finish()
    }
}

/// The signed payload of one custody link.
///
/// `signer_device` is part of the signed bytes, not merely compared against
/// the event after the fact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustodyRecord {
    pub event: CustodyEvent,
    pub signer_device: String,
}

/// Device signing keys trusted to author custody entries.
pub type TrustedCustodyKeys = BTreeMap<String, [u8; 32]>;

/// Clip chain of custody: a [`SignedChain`] of [`CustodyRecord`] payloads.
pub type ProvenanceChain = SignedChain<CustodyRecord>;
/// One link of a [`ProvenanceChain`].
pub type SignedCustodyEntry = ChainLink<CustodyRecord>;

impl ChainEntry for CustodyRecord {
    const DOMAIN: &'static [u8] = b"vbuff-custody-v2";
    const MAX_ENTRIES: usize = MAX_CUSTODY_ENTRIES;
    const LABEL: &'static str = "custody chain";

    type Authority = TrustedCustodyKeys;
    type State = ();

    fn extend_preimage(&self, preimage: &mut Preimage) {
        preimage
            .var(self.signer_device.as_bytes())
            .fixed(&self.event.item_hash)
            .byte(self.event.action.discriminant())
            .var(self.event.device_id.as_bytes())
            .optional(self.event.peer_device.as_deref().map(str::as_bytes))
            .optional(self.event.source_app.as_deref().map(str::as_bytes))
            .u64_be(self.event.timestamp_ms)
            .byte(u8::from(self.event.sensitive));
    }

    /// The one key permitted to sign this entry.
    ///
    /// Enforced identically on append and on verify: the acting device must
    /// be the signer, and its key must be trusted.
    fn expected_signing_key(
        &self,
        _index: usize,
        _state: &(),
        keys: &TrustedCustodyKeys,
    ) -> Result<[u8; 32]> {
        validate_event(&self.event)?;
        if self.signer_device != self.event.device_id {
            return Err(SyncError::Invalid(
                "custody event must be signed by the acting device".into(),
            ));
        }
        keys.get(&self.signer_device)
            .copied()
            .ok_or_else(|| SyncError::Invalid("unknown custody signer".into()))
    }
}

impl ProvenanceChain {
    /// Append an event acted on and signed by `signer_device`.
    pub fn append_event(
        &mut self,
        event: CustodyEvent,
        signer_device: impl Into<String>,
        keys: &TrustedCustodyKeys,
        key: &ed25519_dalek::SigningKey,
    ) -> Result<[u8; 32]> {
        self.append(
            CustodyRecord {
                event,
                signer_device: signer_device.into(),
            },
            keys,
            key,
        )
    }

    #[must_use]
    pub fn sensitive_left_origin(&self) -> bool {
        self.entries.iter().any(|link| {
            link.payload.event.sensitive
                && matches!(
                    link.payload.event.action,
                    CustodyAction::Sent | CustodyAction::Received
                )
        })
    }
}

fn validate_event(event: &CustodyEvent) -> Result<()> {
    let device_ids_valid = is_valid_identifier(&event.device_id, MAX_DEVICE_ID_BYTES)
        && event
            .peer_device
            .as_ref()
            .is_none_or(|peer| is_valid_identifier(peer, MAX_DEVICE_ID_BYTES));
    let source_app_valid = event
        .source_app
        .as_ref()
        .is_none_or(|app| is_valid_label(app, MAX_SOURCE_APP_BYTES));
    if !device_ids_valid || !source_app_valid {
        return Err(SyncError::Invalid("invalid custody event".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::link_preimage;
    use ed25519_dalek::SigningKey;

    fn event() -> CustodyEvent {
        CustodyEvent {
            item_hash: [2; 32],
            action: CustodyAction::Sent,
            device_id: "laptop".into(),
            peer_device: Some("phone".into()),
            source_app: Some("secret.app".into()),
            timestamp_ms: 10,
            sensitive: true,
        }
    }

    fn record() -> CustodyRecord {
        CustodyRecord {
            event: event(),
            signer_device: "laptop".into(),
        }
    }

    fn laptop_keys(key: &SigningKey) -> TrustedCustodyKeys {
        TrustedCustodyKeys::from([("laptop".into(), key.verifying_key().to_bytes())])
    }

    #[test]
    fn custody_link_preimage_is_pinned() {
        // Hand-derived layout: domain, NUL, previous hash, length-prefixed
        // signer, item hash, action tag, length-prefixed device, optional
        // peer, optional source app, timestamp BE u64, sensitive flag.
        let mut expected = Vec::new();
        expected.extend_from_slice(b"vbuff-custody-v2");
        expected.push(0);
        expected.extend_from_slice(&[7; 32]);
        expected.extend_from_slice(&6_u32.to_be_bytes());
        expected.extend_from_slice(b"laptop");
        expected.extend_from_slice(&[2; 32]);
        expected.push(2);
        expected.extend_from_slice(&6_u32.to_be_bytes());
        expected.extend_from_slice(b"laptop");
        expected.push(1);
        expected.extend_from_slice(&5_u32.to_be_bytes());
        expected.extend_from_slice(b"phone");
        expected.push(1);
        expected.extend_from_slice(&10_u32.to_be_bytes());
        expected.extend_from_slice(b"secret.app");
        expected.extend_from_slice(&10_u64.to_be_bytes());
        expected.push(1);
        assert_eq!(link_preimage(&record(), &[7; 32]).unwrap(), expected);

        // An absent optional is one byte and cannot be confused with an
        // empty one.
        let mut absent = record();
        absent.event.peer_device = None;
        let mut empty = record();
        empty.event.peer_device = Some(String::new());
        assert_ne!(
            link_preimage(&absent, &[7; 32]).unwrap(),
            link_preimage(&empty, &[7; 32]).unwrap()
        );
    }

    #[test]
    fn stale_v1_entries_fail_to_deserialize() {
        // v1 stored `event` and `signer_device` next to the hashes.
        let key = SigningKey::from_bytes(&[6; 32]);
        let mut chain = ProvenanceChain::default();
        chain
            .append_event(event(), "laptop", &laptop_keys(&key), &key)
            .unwrap();
        let mut value = serde_json::to_value(&chain).unwrap();
        for link in value["entries"].as_array_mut().unwrap() {
            let payload = link.as_object_mut().unwrap().remove("payload").unwrap();
            for (field, content) in payload.as_object().unwrap() {
                link[field.as_str()] = content.clone();
            }
        }
        let v1 = serde_json::to_string(&value).unwrap();
        assert!(serde_json::from_str::<ProvenanceChain>(&v1).is_err());
    }

    #[test]
    fn custody_chain_is_signed_redacted_and_flags_sensitive_travel() {
        let key = SigningKey::from_bytes(&[6; 32]);
        let keys = laptop_keys(&key);
        let mut chain = ProvenanceChain::default();
        chain.append_event(event(), "laptop", &keys, &key).unwrap();
        chain.verify(&keys).unwrap();
        assert!(chain.sensitive_left_origin());
        assert!(!format!("{:?}", chain.entries[0].payload.event).contains("secret.app"));
        chain.entries[0].payload.event.timestamp_ms = 11;
        assert!(chain.verify(&keys).is_err());
    }

    #[test]
    fn forged_signer_with_a_recomputed_hash_is_rejected() {
        let key = SigningKey::from_bytes(&[6; 32]);
        let ghost = SigningKey::from_bytes(&[5; 32]);
        let mut keys = laptop_keys(&key);
        keys.insert("ghost".into(), ghost.verifying_key().to_bytes());
        let mut chain = ProvenanceChain::default();
        chain.append_event(event(), "laptop", &keys, &key).unwrap();

        let link = &mut chain.entries[0];
        link.payload.signer_device = "ghost".into();
        link.payload.event.device_id = "ghost".into();
        let preimage = link_preimage(&link.payload, &link.previous_hash).unwrap();
        link.hash = *blake3::hash(&preimage).as_bytes();
        // Hash chain intact, signature still the laptop's: rejected.
        assert!(chain.verify(&keys).is_err());
    }

    #[test]
    fn an_untrusted_signer_cannot_append_and_the_chain_is_untouched() {
        let key = SigningKey::from_bytes(&[6; 32]);
        let keys = laptop_keys(&key);
        let mut chain = ProvenanceChain::default();
        chain.append_event(event(), "laptop", &keys, &key).unwrap();
        let before = chain.clone();

        let stranger = SigningKey::from_bytes(&[9; 32]);
        // Right name, wrong key.
        assert!(
            chain
                .append_event(event(), "laptop", &keys, &stranger)
                .is_err()
        );
        // A signer that is not the acting device.
        assert!(chain.append_event(event(), "phone", &keys, &key).is_err());
        // A device with no trusted key at all.
        let mut foreign = event();
        foreign.device_id = "desktop".into();
        assert!(
            chain
                .append_event(foreign, "desktop", &keys, &stranger)
                .is_err()
        );
        assert_eq!(chain, before);
        chain.verify(&keys).unwrap();

        // Revoking the key rejects the whole history on read.
        assert!(chain.verify(&TrustedCustodyKeys::new()).is_err());
    }

    #[test]
    fn identifiers_are_validated() {
        let key = SigningKey::from_bytes(&[6; 32]);
        let mut keys = laptop_keys(&key);
        let mut chain = ProvenanceChain::default();
        let overlong = "x".repeat(MAX_DEVICE_ID_BYTES + 1);
        for bad in ["", "has space", "устройство", overlong.as_str()] {
            let mut peer = event();
            peer.peer_device = Some(bad.into());
            assert!(chain.append_event(peer, "laptop", &keys, &key).is_err());

            keys.insert(bad.into(), key.verifying_key().to_bytes());
            let mut acting = event();
            acting.device_id = bad.into();
            assert!(chain.append_event(acting, bad, &keys, &key).is_err());
        }
        let mut noisy = event();
        noisy.source_app = Some("x".repeat(MAX_SOURCE_APP_BYTES + 1));
        assert!(chain.append_event(noisy, "laptop", &keys, &key).is_err());
        assert!(chain.is_empty());
    }

    #[test]
    fn chain_is_bounded() {
        let key = SigningKey::from_bytes(&[6; 32]);
        let keys = laptop_keys(&key);
        let mut chain = ProvenanceChain::default();
        chain.append_event(event(), "laptop", &keys, &key).unwrap();

        // Filling by hand keeps the test cheap; the bound is checked before
        // anything else, so the links need not be valid.
        let link = chain.entries[0].clone();
        chain.entries.resize(MAX_CUSTODY_ENTRIES, link.clone());
        let full = chain
            .append_event(event(), "laptop", &keys, &key)
            .unwrap_err();
        assert!(full.to_string().contains("custody chain is full"), "{full}");
        assert_eq!(chain.len(), MAX_CUSTODY_ENTRIES);

        chain.entries.push(link);
        let over = chain.verify(&keys).unwrap_err();
        assert!(
            over.to_string().contains("exceeds the entry limit"),
            "{over}"
        );
    }
}
