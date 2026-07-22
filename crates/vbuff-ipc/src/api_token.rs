use std::collections::BTreeSet;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use thiserror::Error;
use zeroize::Zeroize;

use crate::integration::ClipAccessFilter;

type HmacSha256 = Hmac<Sha256>;
const MAX_TOKEN_BYTES: usize = 2_048;
const MAX_TOKEN_TTL_MS: u64 = 30 * 24 * 60 * 60 * 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiScope {
    ReadHistory,
    ReadSensitiveHistory,
    WriteHistory,
    SubscribeEvents,
    RunTransforms,
    Diagnostics,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiTokenClaims {
    pub token_id: [u8; 16],
    pub scopes: BTreeSet<ApiScope>,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    #[serde(default)]
    pub filter: ClipAccessFilter,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ApiTokenError {
    #[error("randomness unavailable")]
    Randomness,
    #[error("token scope set is empty")]
    EmptyScopes,
    #[error("token has invalid framing or encoding")]
    InvalidToken,
    #[error("token signature is invalid")]
    InvalidSignature,
    #[error("token has expired")]
    Expired,
    #[error("token is not valid yet")]
    NotYetValid,
    #[error("token lifetime exceeds the maximum")]
    TtlTooLong,
    #[error("token lacks a required scope")]
    MissingScope,
}

pub struct ApiTokenIssuer {
    signing_key: [u8; 32],
}

impl ApiTokenIssuer {
    pub fn random() -> Result<Self, ApiTokenError> {
        let mut signing_key = [0_u8; 32];
        getrandom::fill(&mut signing_key).map_err(|_| ApiTokenError::Randomness)?;
        Ok(Self { signing_key })
    }

    pub fn from_key(signing_key: [u8; 32]) -> Self {
        Self { signing_key }
    }

    pub fn issue(
        &self,
        scopes: BTreeSet<ApiScope>,
        issued_at_ms: u64,
        ttl_ms: u64,
    ) -> Result<String, ApiTokenError> {
        self.issue_filtered(scopes, ClipAccessFilter::default(), issued_at_ms, ttl_ms)
    }

    pub fn issue_filtered(
        &self,
        scopes: BTreeSet<ApiScope>,
        filter: ClipAccessFilter,
        issued_at_ms: u64,
        ttl_ms: u64,
    ) -> Result<String, ApiTokenError> {
        if scopes.is_empty() {
            return Err(ApiTokenError::EmptyScopes);
        }
        if ttl_ms == 0 || ttl_ms > MAX_TOKEN_TTL_MS {
            return Err(ApiTokenError::TtlTooLong);
        }
        let mut token_id = [0_u8; 16];
        getrandom::fill(&mut token_id).map_err(|_| ApiTokenError::Randomness)?;
        let claims = ApiTokenClaims {
            token_id,
            scopes,
            issued_at_ms,
            expires_at_ms: issued_at_ms
                .checked_add(ttl_ms)
                .ok_or(ApiTokenError::TtlTooLong)?,
            filter,
        };
        self.encode(&claims)
    }

    pub fn verify(
        &self,
        token: &str,
        required_scope: ApiScope,
        now_ms: u64,
    ) -> Result<ApiTokenClaims, ApiTokenError> {
        if token.len() > MAX_TOKEN_BYTES {
            return Err(ApiTokenError::InvalidToken);
        }
        let mut parts = token.split('.');
        if parts.next() != Some("v1") {
            return Err(ApiTokenError::InvalidToken);
        }
        let payload = parts.next().ok_or(ApiTokenError::InvalidToken)?;
        let signature = parts.next().ok_or(ApiTokenError::InvalidToken)?;
        if parts.next().is_some() || payload.len() > MAX_TOKEN_BYTES || signature.len() > 128 {
            return Err(ApiTokenError::InvalidToken);
        }
        let signature = URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|_| ApiTokenError::InvalidToken)?;
        let mut mac = HmacSha256::new_from_slice(&self.signing_key)
            .map_err(|_| ApiTokenError::InvalidToken)?;
        mac.update(b"vbuff-local-api-v1.");
        mac.update(payload.as_bytes());
        mac.verify_slice(&signature)
            .map_err(|_| ApiTokenError::InvalidSignature)?;
        let claims: ApiTokenClaims = serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(payload)
                .map_err(|_| ApiTokenError::InvalidToken)?,
        )
        .map_err(|_| ApiTokenError::InvalidToken)?;
        let lifetime_ms = claims
            .expires_at_ms
            .checked_sub(claims.issued_at_ms)
            .ok_or(ApiTokenError::InvalidToken)?;
        if lifetime_ms == 0 {
            return Err(ApiTokenError::InvalidToken);
        }
        if lifetime_ms > MAX_TOKEN_TTL_MS {
            return Err(ApiTokenError::TtlTooLong);
        }
        if now_ms < claims.issued_at_ms {
            return Err(ApiTokenError::NotYetValid);
        }
        if now_ms >= claims.expires_at_ms {
            return Err(ApiTokenError::Expired);
        }
        if !claims.scopes.contains(&required_scope) {
            return Err(ApiTokenError::MissingScope);
        }
        Ok(claims)
    }

    fn encode(&self, claims: &ApiTokenClaims) -> Result<String, ApiTokenError> {
        let payload = URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(claims).map_err(|_| ApiTokenError::InvalidToken)?);
        let mut mac = HmacSha256::new_from_slice(&self.signing_key)
            .map_err(|_| ApiTokenError::InvalidToken)?;
        mac.update(b"vbuff-local-api-v1.");
        mac.update(payload.as_bytes());
        let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        Ok(format!("v1.{payload}.{signature}"))
    }
}

impl std::fmt::Debug for ApiTokenIssuer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ApiTokenIssuer([redacted])")
    }
}

impl Drop for ApiTokenIssuer {
    fn drop(&mut self) {
        self.signing_key.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_types::ContentKind;

    #[test]
    fn tokens_are_scoped_signed_and_expiring() {
        let issuer = ApiTokenIssuer::from_key([7; 32]);
        let token = issuer
            .issue(BTreeSet::from([ApiScope::ReadHistory]), 100, 50)
            .unwrap();
        assert!(issuer.verify(&token, ApiScope::ReadHistory, 149).is_ok());
        assert_eq!(
            issuer.verify(&token, ApiScope::WriteHistory, 149),
            Err(ApiTokenError::MissingScope)
        );
        assert_eq!(
            issuer.verify(&token, ApiScope::ReadHistory, 150),
            Err(ApiTokenError::Expired)
        );
        assert_eq!(
            issuer.verify(&token, ApiScope::ReadHistory, 99),
            Err(ApiTokenError::NotYetValid)
        );
        let parts = token.split('.').collect::<Vec<_>>();
        let mut payload = parts[1].as_bytes().to_vec();
        payload[0] = if payload[0] == b'A' { b'B' } else { b'A' };
        let tampered = format!("v1.{}.{}", std::str::from_utf8(&payload).unwrap(), parts[2]);
        assert_eq!(
            issuer.verify(&tampered, ApiScope::ReadHistory, 120),
            Err(ApiTokenError::InvalidSignature)
        );
        assert_eq!(
            issuer.verify(
                &format!("v1.{}.signature", "a".repeat(MAX_TOKEN_BYTES)),
                ApiScope::ReadHistory,
                120
            ),
            Err(ApiTokenError::InvalidToken)
        );
        assert_eq!(
            issuer.issue(
                BTreeSet::from([ApiScope::ReadHistory]),
                100,
                MAX_TOKEN_TTL_MS + 1
            ),
            Err(ApiTokenError::TtlTooLong)
        );

        let overlong_claims = ApiTokenClaims {
            token_id: [9; 16],
            scopes: BTreeSet::from([ApiScope::ReadHistory]),
            issued_at_ms: 100,
            expires_at_ms: 100 + MAX_TOKEN_TTL_MS + 1,
            filter: ClipAccessFilter::default(),
        };
        let overlong = issuer.encode(&overlong_claims).unwrap();
        assert_eq!(
            issuer.verify(&overlong, ApiScope::ReadHistory, 101),
            Err(ApiTokenError::TtlTooLong)
        );

        let filtered = issuer
            .issue_filtered(
                BTreeSet::from([ApiScope::ReadHistory]),
                ClipAccessFilter {
                    kinds: BTreeSet::from([ContentKind::Text]),
                    tags: BTreeSet::from(["shareable".into()]),
                    collections: BTreeSet::new(),
                },
                100,
                50,
            )
            .unwrap();
        let claims = issuer
            .verify(&filtered, ApiScope::ReadHistory, 120)
            .unwrap();
        assert!(claims.filter.kinds.contains(&ContentKind::Text));
    }
}
