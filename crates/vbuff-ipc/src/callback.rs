//! One-shot tokens for x-callback-style `vbuff://` automation.

use std::collections::BTreeSet;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use thiserror::Error;
use url::Url;
use zeroize::Zeroize;

type HmacSha256 = Hmac<Sha256>;

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
    #[error("callback token lifetime is invalid")]
    InvalidTtl,
    #[error("randomness is unavailable")]
    Randomness,
}

pub struct CallbackTokenIssuer {
    key: [u8; 32],
    consumed: BTreeSet<[u8; 16]>,
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
            consumed: BTreeSet::new(),
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

    pub fn verify_and_consume(
        &mut self,
        token: &str,
        target: &CallbackTarget,
        now_ms: u64,
    ) -> Result<(), CallbackError> {
        let claims = decode(&self.key, token)?;
        if now_ms < claims.issued_at_ms || now_ms >= claims.expires_at_ms {
            return Err(CallbackError::Expired);
        }
        if claims.target_hash != target.binding_hash()? {
            return Err(CallbackError::TargetMismatch);
        }
        if !self.consumed.insert(claims.nonce) {
            return Err(CallbackError::Replayed);
        }
        Ok(())
    }
}

impl std::fmt::Debug for CallbackTokenIssuer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CallbackTokenIssuer")
            .field("key", &"[redacted]")
            .field("consumed", &self.consumed.len())
            .finish()
    }
}

impl Drop for CallbackTokenIssuer {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

fn encode(key: &[u8; 32], claims: &CallbackClaims) -> Result<String, CallbackError> {
    let payload = URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(claims).map_err(|_| CallbackError::InvalidToken)?);
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| CallbackError::InvalidToken)?;
    mac.update(b"vbuff-x-callback-v1.");
    mac.update(payload.as_bytes());
    Ok(format!(
        "v1.{payload}.{}",
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    ))
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
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| CallbackError::InvalidToken)?;
    mac.update(b"vbuff-x-callback-v1.");
    mac.update(payload.as_bytes());
    mac.verify_slice(&signature)
        .map_err(|_| CallbackError::InvalidSignature)?;
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
