use std::collections::BTreeMap;
use std::fmt;

use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use super::{IntegrationContractError, valid_identifier};

const MAX_REMOTE_REPLAY_ENTRIES: usize = 4_096;
type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemotePasteRequest {
    pub forwarded_socket: String,
    pub session_nonce: String,
    pub clip_id: String,
}

impl fmt::Debug for RemotePasteRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemotePasteRequest")
            .field("forwarded_socket_bytes", &self.forwarded_socket.len())
            .field("session_nonce", &"[redacted]")
            .field("clip_id", &"[redacted]")
            .finish()
    }
}

impl RemotePasteRequest {
    pub fn validate(&self) -> Result<(), IntegrationContractError> {
        if !valid_forwarded_socket(&self.forwarded_socket)
            || !valid_identifier(&self.session_nonce, 128)
            || !valid_identifier(&self.clip_id, 128)
        {
            return Err(IntegrationContractError::InvalidField);
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemotePasteLease {
    pub request_hash: [u8; 32],
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    proof: [u8; 32],
}

impl fmt::Debug for RemotePasteLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemotePasteLease")
            .field("request_hash", &"[redacted]")
            .field("issued_at_ms", &self.issued_at_ms)
            .field("expires_at_ms", &self.expires_at_ms)
            .field("proof", &"[redacted]")
            .finish()
    }
}

impl RemotePasteLease {
    pub fn bind(
        request: &RemotePasteRequest,
        session_key: &[u8; 32],
        issued_at_ms: u64,
        ttl_ms: u64,
    ) -> Result<Self, IntegrationContractError> {
        request.validate()?;
        if session_key.iter().all(|byte| *byte == 0) || ttl_ms == 0 || ttl_ms > 60_000 {
            return Err(IntegrationContractError::InvalidField);
        }
        let expires_at_ms = issued_at_ms
            .checked_add(ttl_ms)
            .ok_or(IntegrationContractError::InvalidField)?;
        let request_hash = request_hash(request)?;
        let proof = remote_proof(session_key, &request_hash, issued_at_ms, expires_at_ms)?;
        Ok(Self {
            request_hash,
            issued_at_ms,
            expires_at_ms,
            proof,
        })
    }
}

#[derive(Clone, Default)]
pub struct RemoteReplayWindow {
    consumed: BTreeMap<[u8; 32], u64>,
}

impl fmt::Debug for RemoteReplayWindow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemoteReplayWindow")
            .field("consumed_count", &self.consumed.len())
            .finish()
    }
}

impl RemoteReplayWindow {
    pub fn verify_and_consume(
        &mut self,
        lease: &RemotePasteLease,
        request: &RemotePasteRequest,
        session_key: &[u8; 32],
        now_ms: u64,
    ) -> Result<(), IntegrationContractError> {
        self.consumed.retain(|_, expiry| *expiry > now_ms);
        request.validate()?;
        if session_key.iter().all(|byte| *byte == 0) {
            return Err(IntegrationContractError::InvalidField);
        }
        if now_ms < lease.issued_at_ms || now_ms >= lease.expires_at_ms {
            return Err(IntegrationContractError::Expired);
        }
        let request_hash = request_hash(request)?;
        if request_hash != lease.request_hash
            || self.consumed.contains_key(&request_hash)
            || self.consumed.len() >= MAX_REMOTE_REPLAY_ENTRIES
            || !verify_remote_proof(
                session_key,
                &request_hash,
                lease.issued_at_ms,
                lease.expires_at_ms,
                &lease.proof,
            )
        {
            return Err(IntegrationContractError::InvalidField);
        }
        self.consumed.insert(request_hash, lease.expires_at_ms);
        Ok(())
    }
}

fn request_hash(request: &RemotePasteRequest) -> Result<[u8; 32], IntegrationContractError> {
    serde_json::to_vec(request)
        .map(|bytes| *blake3::hash(&bytes).as_bytes())
        .map_err(|_| IntegrationContractError::InvalidField)
}

fn remote_proof(
    key: &[u8; 32],
    request_hash: &[u8; 32],
    issued_at_ms: u64,
    expires_at_ms: u64,
) -> Result<[u8; 32], IntegrationContractError> {
    if key.iter().all(|byte| *byte == 0) {
        return Err(IntegrationContractError::InvalidField);
    }
    let mut mac =
        HmacSha256::new_from_slice(key).map_err(|_| IntegrationContractError::InvalidField)?;
    mac.update(b"vbuff-remote-paste-v1");
    mac.update(request_hash);
    mac.update(&issued_at_ms.to_be_bytes());
    mac.update(&expires_at_ms.to_be_bytes());
    Ok(mac.finalize().into_bytes().into())
}

fn verify_remote_proof(
    key: &[u8; 32],
    request_hash: &[u8; 32],
    issued_at_ms: u64,
    expires_at_ms: u64,
    proof: &[u8; 32],
) -> bool {
    let Ok(mut mac) = HmacSha256::new_from_slice(key) else {
        return false;
    };
    mac.update(b"vbuff-remote-paste-v1");
    mac.update(request_hash);
    mac.update(&issued_at_ms.to_be_bytes());
    mac.update(&expires_at_ms.to_be_bytes());
    mac.verify_slice(proof).is_ok()
}

fn valid_forwarded_socket(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.contains("//")
        && !value
            .split('/')
            .any(|segment| matches!(segment, "." | ".."))
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/' | b':')
        })
}
