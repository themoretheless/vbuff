//! One-shot tokens for x-callback-style `vbuff://` automation.

use std::collections::BTreeSet;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;
use vbuff_types::mac::{MacDomain, MacProof, hmac_proof};
use zeroize::Zeroize;

use crate::replay::{MAX_REPLAY_ENTRIES, ReplayGuard};

/// Frozen framing: the `.` was baked into the domain constant when these
/// tokens started being issued. It is a genuine separator, because the payload
/// that follows is base64url and `.` is outside that alphabet. Moving to the
/// NUL convention would invalidate every live token for one `MAX_TTL_MS`
/// window and buy nothing; see `docs/domain-separation-convention.md` §6.1.
const CALLBACK_DOMAIN: MacDomain = MacDomain::legacy_ascii_separated("vbuff-x-callback-v1", b'.');

const MAX_URI_BYTES: usize = 8 * 1024;
const MAX_TOKEN_BYTES: usize = 2 * 1024;
const MAX_CALLBACK_BYTES: usize = 2 * 1024;
const MAX_TTL_MS: u64 = 10 * 60 * 1_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransformAction {
    Base64Encode,
    Base64Decode,
    Trim,
    PlainText,
}

impl TransformAction {
    fn parse(value: &str) -> Result<Self, CallbackError> {
        match value {
            "base64_encode" | "base64" => Ok(Self::Base64Encode),
            "base64_decode" => Ok(Self::Base64Decode),
            "trim" => Ok(Self::Trim),
            "plain_text" => Ok(Self::PlainText),
            _ => Err(CallbackError::UnsupportedAction),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallbackTarget {
    pub action: TransformAction,
    pub success_url: String,
    pub error_url: Option<String>,
}

impl CallbackTarget {
    pub fn new(
        action: TransformAction,
        success_url: impl Into<String>,
        error_url: Option<String>,
    ) -> Result<Self, CallbackError> {
        let target = Self {
            action,
            success_url: success_url.into(),
            error_url,
        };
        validate_callback_url(&target.success_url)?;
        if let Some(url) = &target.error_url {
            validate_callback_url(url)?;
        }
        Ok(target)
    }

    fn binding_hash(&self) -> Result<[u8; 32], CallbackError> {
        let bytes = serde_json::to_vec(self).map_err(|_| CallbackError::InvalidToken)?;
        Ok(*blake3::hash(&bytes).as_bytes())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallbackInvocation {
    pub target: CallbackTarget,
    pub token: String,
}

impl CallbackInvocation {
    pub fn parse(uri: &str) -> Result<Self, CallbackError> {
        if uri.len() > MAX_URI_BYTES {
            return Err(CallbackError::InvalidUri);
        }
        let url = Url::parse(uri).map_err(|_| CallbackError::InvalidUri)?;
        if url.scheme() != "vbuff" || url.host_str() != Some("transform") {
            return Err(CallbackError::InvalidUri);
        }
        let mut op = None;
        let mut success = None;
        let mut error = None;
        let mut token = None;
        let mut seen = BTreeSet::new();
        for (key, value) in url.query_pairs() {
            if !seen.insert(key.to_string()) {
                return Err(CallbackError::DuplicateParameter);
            }
            match key.as_ref() {
                "op" => op = Some(TransformAction::parse(&value)?),
                "x-success" => success = Some(value.into_owned()),
                "x-error" => error = Some(value.into_owned()),
                "token" => token = Some(value.into_owned()),
                _ => return Err(CallbackError::InvalidUri),
            }
        }
        let target = CallbackTarget::new(
            op.ok_or(CallbackError::InvalidUri)?,
            success.ok_or(CallbackError::InvalidUri)?,
            error,
        )?;
        let token = token.ok_or(CallbackError::InvalidUri)?;
        if token.len() > MAX_TOKEN_BYTES {
            return Err(CallbackError::InvalidToken);
        }
        Ok(Self { target, token })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct CallbackClaims {
    nonce: [u8; 16],
    target_hash: [u8; 32],
    issued_at_ms: u64,
    expires_at_ms: u64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CallbackError {
    #[error("callback URI is invalid or too large")]
    InvalidUri,
    #[error("callback URI contains a duplicate parameter")]
    DuplicateParameter,
    #[error("callback action is unsupported")]
    UnsupportedAction,
    #[error("callback target scheme is unsafe")]
    UnsafeCallback,
    #[error("callback token is invalid")]
    InvalidToken,
    #[error("callback token signature is invalid")]
    InvalidSignature,
    #[error("callback token is expired or not active yet")]
    Expired,
    #[error("callback token does not match this action")]
    TargetMismatch,
    #[error("callback token has already been consumed")]
    Replayed,
    #[error("callback replay window is saturated")]
    ReplayWindowFull,
    #[error("callback token lifetime is invalid")]
    InvalidTtl,
    #[error("randomness is unavailable")]
    Randomness,
}

pub struct CallbackTokenIssuer {
    key: [u8; 32],
    /// Nonces of consumed tokens, each burned until the expiry of the token
    /// that carried it, so the window drops an entry the moment it stops
    /// mattering.
    replay: ReplayGuard<[u8; 16]>,
}

impl CallbackTokenIssuer {
    pub fn random() -> Result<Self, CallbackError> {
        let mut key = [0_u8; 32];
        getrandom::fill(&mut key).map_err(|_| CallbackError::Randomness)?;
        Ok(Self::from_key(key))
    }

    pub fn from_key(key: [u8; 32]) -> Self {
        Self {
            key,
            replay: ReplayGuard::new(MAX_REPLAY_ENTRIES),
        }
    }

    pub fn issue(
        &self,
        target: &CallbackTarget,
        issued_at_ms: u64,
        ttl_ms: u64,
    ) -> Result<String, CallbackError> {
        if ttl_ms == 0 || ttl_ms > MAX_TTL_MS {
            return Err(CallbackError::InvalidTtl);
        }
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).map_err(|_| CallbackError::Randomness)?;
        let claims = CallbackClaims {
            nonce,
            target_hash: target.binding_hash()?,
            issued_at_ms,
            expires_at_ms: issued_at_ms
                .checked_add(ttl_ms)
                .ok_or(CallbackError::InvalidTtl)?,
        };
        encode(&self.key, &claims)
    }

    /// Verifies a one-shot token and burns its nonce.
    ///
    /// The nonce is burned until the token's own `expires_at_ms`, which is
    /// exactly the instant where the freshness check above starts answering
    /// [`CallbackError::Expired`], so the guard's eviction rule (see
    /// [`ReplayGuard::advance_to`]) never resurrects a token: inside the window
    /// the replay is refused with [`CallbackError::Replayed`], and from the
    /// moment the entry is dropped the clamped clock refuses it as expired.
    ///
    /// Memory is therefore bounded by the issuance rate over `MAX_TTL_MS`, and
    /// hard-capped by `MAX_REPLAY_ENTRIES` fail-closed.
    pub fn verify_and_consume(
        &mut self,
        token: &str,
        target: &CallbackTarget,
        now_ms: u64,
    ) -> Result<(), CallbackError> {
        let now_ms = self.replay.advance_to(now_ms);
        let claims = decode(&self.key, token)?;
        if now_ms < claims.issued_at_ms || now_ms >= claims.expires_at_ms {
            return Err(CallbackError::Expired);
        }
        if claims.target_hash != target.binding_hash()? {
            return Err(CallbackError::TargetMismatch);
        }
        if self.replay.contains(&claims.nonce) {
            return Err(CallbackError::Replayed);
        }
        self.replay
            .burn(claims.nonce, claims.expires_at_ms)
            .map_err(|_| CallbackError::ReplayWindowFull)
    }
}

impl std::fmt::Debug for CallbackTokenIssuer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CallbackTokenIssuer")
            .field("key", &"[redacted]")
            .field("consumed", &self.replay.len())
            .finish()
    }
}

impl Drop for CallbackTokenIssuer {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

/// The one statement of what a callback token's MAC covers: the base64url
/// payload, and nothing else. [`encode`] and [`decode`] both reach their tag
/// through here, so neither can start covering a different message than the
/// other.
fn token_proof(key: &[u8; 32], payload: &str) -> MacProof {
    hmac_proof(CALLBACK_DOMAIN, key, &[payload.as_bytes()])
}

fn encode(key: &[u8; 32], claims: &CallbackClaims) -> Result<String, CallbackError> {
    let payload = URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(claims).map_err(|_| CallbackError::InvalidToken)?);
    let signature = URL_SAFE_NO_PAD.encode(token_proof(key, &payload).finish());
    Ok(format!("v1.{payload}.{signature}"))
}

fn decode(key: &[u8; 32], token: &str) -> Result<CallbackClaims, CallbackError> {
    if token.len() > MAX_TOKEN_BYTES {
        return Err(CallbackError::InvalidToken);
    }
    let mut parts = token.split('.');
    if parts.next() != Some("v1") {
        return Err(CallbackError::InvalidToken);
    }
    let payload = parts.next().ok_or(CallbackError::InvalidToken)?;
    let signature = parts.next().ok_or(CallbackError::InvalidToken)?;
    if parts.next().is_some() {
        return Err(CallbackError::InvalidToken);
    }
    let signature = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| CallbackError::InvalidToken)?;
    if !token_proof(key, payload).verify(&signature) {
        return Err(CallbackError::InvalidSignature);
    }
    serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| CallbackError::InvalidToken)?,
    )
    .map_err(|_| CallbackError::InvalidToken)
}

fn validate_callback_url(value: &str) -> Result<(), CallbackError> {
    if value.is_empty() || value.len() > MAX_CALLBACK_BYTES {
        return Err(CallbackError::UnsafeCallback);
    }
    let url = Url::parse(value).map_err(|_| CallbackError::UnsafeCallback)?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(CallbackError::UnsafeCallback);
    }
    let scheme = url.scheme();
    let allowed = matches!(
        scheme,
        "https" | "shortcuts" | "things" | "bear" | "drafts" | "obsidian"
    );
    if !allowed || (scheme == "https" && url.host_str().is_none()) {
        return Err(CallbackError::UnsafeCallback);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hex, legacy_mac};

    /// Freeze test for the callback token MAC.
    ///
    /// These bytes are handed out to whoever holds a live token, so the pin is
    /// not tidiness: if it fails, some edit has silently invalidated every
    /// token issued in the last `MAX_TTL_MS`.
    #[test]
    fn callback_mac_bytes_are_frozen_and_domain_bound() {
        let key = [9_u8; 32];
        let claims = CallbackClaims {
            nonce: [1; 16],
            target_hash: [2; 32],
            issued_at_ms: 100,
            expires_at_ms: 200,
        };
        let token = encode(&key, &claims).unwrap();
        let payload = token.split('.').nth(1).unwrap();
        let tag = token_proof(&key, payload).finish();

        assert_eq!(
            hex(&tag),
            "44cb8d4b9045a626e9fa90c109e0d1f866dd12c0e0d1f2e2587dbe99885bfb8a"
        );
        // Byte-identical to the hand-rolled `mac.update(b"vbuff-x-callback-v1.")`
        // pair this replaced.
        assert_eq!(
            tag,
            legacy_mac(b"vbuff-x-callback-v1.", &key, &[payload.as_bytes()])
        );
        assert!(token_proof(&key, payload).verify(&tag));

        // Domain separation is real in both directions: the same label under
        // the NUL convention, and a different label, both produce tags this
        // mechanism refuses.
        for foreign in [
            hmac_proof(
                MacDomain::new("vbuff-x-callback-v1"),
                &key,
                &[payload.as_bytes()],
            )
            .finish(),
            hmac_proof(
                MacDomain::legacy_ascii_separated("vbuff-local-api-v1", b'.'),
                &key,
                &[payload.as_bytes()],
            )
            .finish(),
        ] {
            assert_ne!(foreign, tag);
            assert!(!token_proof(&key, payload).verify(&foreign));
        }
    }

    #[test]
    fn a_token_signed_under_a_foreign_domain_is_rejected_end_to_end() {
        let key = [9_u8; 32];
        let target =
            CallbackTarget::new(TransformAction::Trim, "bear://x-callback-url/create", None)
                .unwrap();
        let claims = CallbackClaims {
            nonce: [4; 16],
            target_hash: target.binding_hash().unwrap(),
            issued_at_ms: 100,
            expires_at_ms: 200,
        };
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
        let forged = URL_SAFE_NO_PAD.encode(
            hmac_proof(
                MacDomain::new("vbuff-x-callback-v1"),
                &key,
                &[payload.as_bytes()],
            )
            .finish(),
        );
        let mut issuer = CallbackTokenIssuer::from_key(key);
        assert_eq!(
            issuer.verify_and_consume(&format!("v1.{payload}.{forged}"), &target, 150),
            Err(CallbackError::InvalidSignature)
        );
    }

    #[test]
    fn invocation_is_bound_one_shot_and_scheme_safe() {
        let target = CallbackTarget::new(
            TransformAction::Trim,
            "shortcuts://run-shortcut?name=receive",
            Some("https://example.test/error".into()),
        )
        .unwrap();
        let mut issuer = CallbackTokenIssuer::from_key([9; 32]);
        let token = issuer.issue(&target, 100, 50).unwrap();
        issuer.verify_and_consume(&token, &target, 120).unwrap();
        assert_eq!(
            issuer.verify_and_consume(&token, &target, 120),
            Err(CallbackError::Replayed)
        );

        let changed = CallbackTarget::new(
            TransformAction::PlainText,
            "shortcuts://run-shortcut?name=receive",
            None,
        )
        .unwrap();
        let token = issuer.issue(&target, 200, 50).unwrap();
        assert_eq!(
            issuer.verify_and_consume(&token, &changed, 220),
            Err(CallbackError::TargetMismatch)
        );
        assert_eq!(
            CallbackTarget::new(TransformAction::Trim, "javascript:alert(1)", None),
            Err(CallbackError::UnsafeCallback)
        );
        assert_eq!(
            CallbackTarget::new(TransformAction::Trim, "shell://run?command=rm", None),
            Err(CallbackError::UnsafeCallback)
        );
        assert_eq!(
            CallbackTarget::new(TransformAction::Trim, "https://", None),
            Err(CallbackError::UnsafeCallback)
        );
    }

    fn window_target() -> CallbackTarget {
        CallbackTarget::new(TransformAction::Trim, "bear://x-callback-url/create", None).unwrap()
    }

    #[test]
    fn replay_entries_are_evicted_once_they_leave_the_validity_window() {
        let target = window_target();
        let mut issuer = CallbackTokenIssuer::from_key([3; 32]);
        let token = issuer.issue(&target, 0, 100).unwrap();
        issuer.verify_and_consume(&token, &target, 10).unwrap();
        assert_eq!(issuer.replay.len(), 1);

        // One millisecond before expiry the entry must survive cleanup.
        assert_eq!(
            issuer.verify_and_consume(&token, &target, 99),
            Err(CallbackError::Replayed)
        );
        assert_eq!(issuer.replay.len(), 1);

        // At expiry the entry is dropped, and the token is refused on time
        // instead of on the nonce set, so nothing is weakened by the eviction.
        assert_eq!(
            issuer.verify_and_consume(&token, &target, 100),
            Err(CallbackError::Expired)
        );
        assert!(issuer.replay.is_empty());
    }

    #[test]
    fn cleanup_keeps_rejecting_replays_inside_the_window() {
        let target = window_target();
        let mut issuer = CallbackTokenIssuer::from_key([6; 32]);
        let long_lived = issuer.issue(&target, 0, MAX_TTL_MS).unwrap();
        issuer.verify_and_consume(&long_lived, &target, 1).unwrap();

        // Every verification runs cleanup; short-lived neighbours come and go
        // while the long-lived nonce must stay burned for its whole window.
        for step in 2..512 {
            let short_lived = issuer.issue(&target, step, 1).unwrap();
            issuer
                .verify_and_consume(&short_lived, &target, step)
                .unwrap();
            assert_eq!(
                issuer.verify_and_consume(&long_lived, &target, step),
                Err(CallbackError::Replayed)
            );
            assert_eq!(issuer.replay.len(), 2);
        }
        assert_eq!(
            issuer.verify_and_consume(&long_lived, &target, MAX_TTL_MS - 1),
            Err(CallbackError::Replayed)
        );
    }

    #[test]
    fn rewound_clock_cannot_resurrect_an_evicted_nonce() {
        let target = window_target();
        let mut issuer = CallbackTokenIssuer::from_key([8; 32]);
        let token = issuer.issue(&target, 0, 100).unwrap();
        issuer.verify_and_consume(&token, &target, 10).unwrap();

        let later = issuer.issue(&target, 200, 10).unwrap();
        issuer.verify_and_consume(&later, &target, 200).unwrap();
        assert_eq!(issuer.replay.len(), 1);

        assert_eq!(
            issuer.verify_and_consume(&token, &target, 50),
            Err(CallbackError::Expired)
        );
    }

    #[test]
    fn saturated_replay_window_fails_closed() {
        let target = window_target();
        let mut issuer = CallbackTokenIssuer::from_key([5; 32]);
        let token = issuer.issue(&target, 0, 100).unwrap();
        for index in 0..MAX_REPLAY_ENTRIES {
            let mut nonce = [0_u8; 16];
            nonce[..8].copy_from_slice(&(index as u64).to_be_bytes());
            issuer.replay.burn(nonce, 1_000).unwrap();
        }
        assert_eq!(
            issuer.verify_and_consume(&token, &target, 10),
            Err(CallbackError::ReplayWindowFull)
        );

        // Once the saturating entries age out, the window accepts again.
        let token = issuer.issue(&target, 1_000, 100).unwrap();
        issuer.verify_and_consume(&token, &target, 1_000).unwrap();
        assert_eq!(issuer.replay.len(), 1);
    }

    #[test]
    fn uri_parser_rejects_duplicates_and_unknown_actions() {
        let target = CallbackTarget::new(
            TransformAction::Base64Encode,
            "things:///show?id=result",
            None,
        )
        .unwrap();
        let issuer = CallbackTokenIssuer::from_key([4; 32]);
        let token = issuer.issue(&target, 10, 100).unwrap();
        let uri = format!(
            "vbuff://transform?op=base64&x-success=things%3A%2F%2F%2Fshow%3Fid%3Dresult&token={token}"
        );
        assert_eq!(CallbackInvocation::parse(&uri).unwrap().target, target);
        assert_eq!(
            CallbackInvocation::parse(
                "vbuff://transform?op=trim&op=base64&x-success=things:///show&token=x"
            ),
            Err(CallbackError::DuplicateParameter)
        );
    }
}
