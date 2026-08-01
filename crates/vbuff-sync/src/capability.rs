//! Scoped, expiring, one-shot capability tokens.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use vbuff_types::mac::{MacDomain, hmac_proof};
use vbuff_types::replay::{MAX_REPLAY_ENTRIES, ReplayGuard};

use crate::{Result, SyncError};

const MAX_TOKEN_BYTES: usize = 4 * 1024;

/// Domain separating capability tokens from every other MAC in the workspace.
///
/// The `v2` label is not a format tweak: `v1` authenticated the bare JSON
/// payload with no domain at all, so a token was only distinguishable from
/// another mechanism's MAC by the fact that the two never shared a key. Tokens
/// live minutes and no build persists them, so the break costs nothing today.
const CAPABILITY_DOMAIN: MacDomain = MacDomain::new("vbuff-capability-token-v2");

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

/// Authenticated bytes of one token: the domain plus the serialized scope.
///
/// Written once so issuing and verifying can never authenticate different
/// bytes; the payload is the only variable-length part, so no length prefix is
/// needed for the concatenation to be unambiguous.
fn token_proof(secret: &[u8; 32], payload: &[u8]) -> vbuff_types::mac::MacProof {
    hmac_proof(CAPABILITY_DOMAIN, secret, &[payload])
}

pub fn issue(secret: &[u8; 32], mut scope: CapabilityScope) -> Result<String> {
    getrandom::fill(&mut scope.nonce).map_err(|_| SyncError::Crypto)?;
    let payload = serde_json::to_vec(&scope)?;
    let signature = token_proof(secret, &payload).finish();
    Ok(format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(&payload),
        URL_SAFE_NO_PAD.encode(signature)
    ))
}

/// Burnt and revoked nonces, both bounded.
///
/// The two windows use the shared [`ReplayGuard`], so they inherit its
/// monotonic clock (a rewound caller clock cannot resurrect a spent token),
/// its retention rule (an entry is dropped exactly when the token it protects
/// starts failing its own expiry check) and its fail-closed ceiling.
#[derive(Debug)]
pub struct CapabilityVerifier {
    consumed: ReplayGuard<[u8; 16]>,
    revoked: ReplayGuard<[u8; 16]>,
}

impl Default for CapabilityVerifier {
    fn default() -> Self {
        Self {
            consumed: ReplayGuard::new(MAX_REPLAY_ENTRIES),
            revoked: ReplayGuard::new(MAX_REPLAY_ENTRIES),
        }
    }
}

impl CapabilityVerifier {
    pub fn verify_and_consume(
        &mut self,
        secret: &[u8; 32],
        token: &str,
        now_ms: u64,
    ) -> Result<CapabilityScope> {
        // Both windows share the clamped clock, so a rewind cannot un-revoke a
        // token either.
        let now_ms = self.consumed.advance_to(now_ms);
        let now_ms = self.revoked.advance_to(now_ms);
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
        if !token_proof(secret, &payload).verify(&signature) {
            return Err(SyncError::Crypto);
        }
        let scope: CapabilityScope = serde_json::from_slice(&payload)?;
        if scope.expires_at_ms <= now_ms {
            return Err(SyncError::Invalid("capability expired".into()));
        }
        if self.revoked.contains(&scope.nonce) || self.consumed.contains(&scope.nonce) {
            return Err(SyncError::Invalid(
                "capability already consumed or revoked".into(),
            ));
        }
        // Fail closed: a one-shot token that cannot be recorded as spent is a
        // repeatable token, so a saturated window refuses the request.
        self.consumed
            .burn(scope.nonce, scope.expires_at_ms)
            .map_err(|_| SyncError::Invalid("capability replay window is full".into()))?;
        Ok(scope)
    }

    /// Records `nonce` as revoked until `expires_at_ms`.
    ///
    /// Returns an error once the revocation window is saturated rather than
    /// dropping the revocation, which would silently re-enable the token.
    pub fn revoke(&mut self, nonce: [u8; 16], expires_at_ms: u64) -> Result<()> {
        self.revoked
            .burn(nonce, expires_at_ms)
            .map_err(|_| SyncError::Invalid("capability revocation window is full".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(expires_at_ms: u64) -> CapabilityScope {
        CapabilityScope {
            target_device: "phone".into(),
            item_hash: [9; 32],
            action: CapabilityAction::PushOneItem,
            expires_at_ms,
            nonce: [0; 16],
        }
    }

    #[test]
    fn token_is_scoped_expiring_and_one_shot() {
        let key = [5_u8; 32];
        let token = issue(&key, scope(2_000)).unwrap();
        let mut verifier = CapabilityVerifier::default();
        let scope = verifier.verify_and_consume(&key, &token, 1_000).unwrap();
        assert_eq!(scope.target_device, "phone");
        assert!(verifier.verify_and_consume(&key, &token, 1_001).is_err());
        let mut revoked = CapabilityVerifier::default();
        revoked.revoke(scope.nonce, scope.expires_at_ms).unwrap();
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

    #[test]
    fn a_rewound_clock_cannot_resurrect_a_spent_token() {
        let key = [5_u8; 32];
        let token = issue(&key, scope(2_000)).unwrap();
        let mut verifier = CapabilityVerifier::default();
        verifier.verify_and_consume(&key, &token, 1_000).unwrap();
        // Past the expiry the burn record is evicted, which is safe only
        // because the clock never moves back: replaying the token with a
        // rewound clock must still be refused.
        assert!(verifier.verify_and_consume(&key, &token, 2_000).is_err());
        assert!(verifier.verify_and_consume(&key, &token, 1_000).is_err());
    }

    #[test]
    fn spent_nonces_do_not_accumulate_past_their_expiry() {
        let key = [5_u8; 32];
        let mut verifier = CapabilityVerifier::default();
        for step in 0..64 {
            let now = 1_000 + step * 10;
            let token = issue(&key, scope(now + 5)).unwrap();
            verifier.verify_and_consume(&key, &token, now).unwrap();
        }
        // Each token outlives only its own five-millisecond window, so the
        // guard holds at most the tokens issued inside the newest one.
        assert!(verifier.consumed.len() <= 1);
    }

    #[test]
    fn a_saturated_window_refuses_the_token_instead_of_forgetting_it() {
        let key = [5_u8; 32];
        let mut verifier = CapabilityVerifier {
            consumed: ReplayGuard::new(1),
            revoked: ReplayGuard::new(MAX_REPLAY_ENTRIES),
        };
        let first = issue(&key, scope(9_000)).unwrap();
        let second = issue(&key, scope(9_000)).unwrap();
        verifier.verify_and_consume(&key, &first, 1_000).unwrap();
        assert!(verifier.verify_and_consume(&key, &second, 1_000).is_err());
    }

    #[test]
    fn a_token_authenticated_under_another_domain_is_refused() {
        let key = [5_u8; 32];
        let payload = serde_json::to_vec(&scope(2_000)).unwrap();
        // Exactly the v1 construction: the bare payload, no domain at all.
        let forged = hmac_proof(
            vbuff_types::mac::MacDomain::legacy_unterminated(""),
            &key,
            &[&payload],
        )
        .finish();
        let token = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(&payload),
            URL_SAFE_NO_PAD.encode(forged)
        );
        let mut verifier = CapabilityVerifier::default();
        assert!(verifier.verify_and_consume(&key, &token, 1_000).is_err());
    }
}
