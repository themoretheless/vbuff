//! Scoped, expiring, one-shot capability tokens.

use std::collections::BTreeMap;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::{Result, SyncError};

type HmacSha256 = Hmac<Sha256>;
const MAX_TOKEN_BYTES: usize = 4 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityAction {
    PushOneItem,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityScope {
    pub target_device: String,
    pub item_hash: [u8; 32],
    pub action: CapabilityAction,
    pub expires_at_ms: u64,
    pub nonce: [u8; 16],
}

pub fn issue(secret: &[u8; 32], mut scope: CapabilityScope) -> Result<String> {
    getrandom::fill(&mut scope.nonce).map_err(|_| SyncError::Crypto)?;
    let payload = serde_json::to_vec(&scope)?;
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| SyncError::Crypto)?;
    mac.update(&payload);
    let signature = mac.finalize().into_bytes();
    Ok(format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(payload),
        URL_SAFE_NO_PAD.encode(signature)
    ))
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CapabilityVerifier {
    consumed: BTreeMap<[u8; 16], u64>,
    revoked: BTreeMap<[u8; 16], u64>,
}

impl CapabilityVerifier {
    pub fn verify_and_consume(
        &mut self,
        secret: &[u8; 32],
        token: &str,
        now_ms: u64,
    ) -> Result<CapabilityScope> {
        self.consumed.retain(|_, expiry| *expiry > now_ms);
        self.revoked.retain(|_, expiry| *expiry > now_ms);
        if token.len() > MAX_TOKEN_BYTES {
            return Err(SyncError::Invalid("capability token is too large".into()));
        }
        let (payload, signature) = token
            .split_once('.')
            .ok_or_else(|| SyncError::Invalid("malformed capability token".into()))?;
        let payload = URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| SyncError::Invalid("invalid capability payload".into()))?;
        let signature = URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|_| SyncError::Invalid("invalid capability signature".into()))?;
        let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| SyncError::Crypto)?;
        mac.update(&payload);
        mac.verify_slice(&signature)
            .map_err(|_| SyncError::Crypto)?;
        let scope: CapabilityScope = serde_json::from_slice(&payload)?;
        if scope.expires_at_ms <= now_ms {
            return Err(SyncError::Invalid("capability expired".into()));
        }
        if self.revoked.contains_key(&scope.nonce) || self.consumed.contains_key(&scope.nonce) {
            return Err(SyncError::Invalid(
                "capability already consumed or revoked".into(),
            ));
        }
        self.consumed.insert(scope.nonce, scope.expires_at_ms);
        Ok(scope)
    }

    pub fn revoke(&mut self, nonce: [u8; 16], expires_at_ms: u64) {
        self.revoked.insert(nonce, expires_at_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_scoped_expiring_and_one_shot() {
        let key = [5_u8; 32];
        let token = issue(
            &key,
            CapabilityScope {
                target_device: "phone".into(),
                item_hash: [9; 32],
                action: CapabilityAction::PushOneItem,
                expires_at_ms: 2_000,
                nonce: [0; 16],
            },
        )
        .unwrap();
        let mut verifier = CapabilityVerifier::default();
        let scope = verifier.verify_and_consume(&key, &token, 1_000).unwrap();
        assert_eq!(scope.target_device, "phone");
        assert!(verifier.verify_and_consume(&key, &token, 1_001).is_err());
        let mut revoked = CapabilityVerifier::default();
        revoked.revoke(scope.nonce, scope.expires_at_ms);
        assert!(revoked.verify_and_consume(&key, &token, 1_000).is_err());
        let mut at_expiry = CapabilityVerifier::default();
        assert!(at_expiry.verify_and_consume(&key, &token, 2_000).is_err());
        let mut other = CapabilityVerifier::default();
        assert!(other.verify_and_consume(&[6; 32], &token, 1_000).is_err());
        assert!(
            other
                .verify_and_consume(&key, &"a".repeat(MAX_TOKEN_BYTES + 1), 1_000)
                .is_err()
        );
    }
}
