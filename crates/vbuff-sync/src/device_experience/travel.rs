use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

use super::{MAX_QR_TOKEN_TTL_MS, all_zero};
use crate::{Result, SyncError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TravelMode {
    pub enabled: bool,
    pub enabled_at_ms: u64,
    pub expires_at_ms: Option<u64>,
    pub retention_hours: u16,
}

impl TravelMode {
    pub fn validate(self) -> Result<()> {
        if self.retention_hours == 0
            || self.retention_hours > 7 * 24
            || self
                .expires_at_ms
                .is_some_and(|expires| expires <= self.enabled_at_ms)
        {
            return Err(SyncError::Invalid("invalid travel mode".into()));
        }
        Ok(())
    }

    pub fn active(self, now_ms: u64) -> Result<bool> {
        self.validate()?;
        Ok(self.active_unchecked(now_ms))
    }

    pub fn sync_allowed(self, now_ms: u64) -> bool {
        self.active(now_ms).is_ok_and(|active| !active)
    }

    pub fn sensitive_visible(self, now_ms: u64) -> bool {
        self.active(now_ms).is_ok_and(|active| !active)
    }

    fn active_unchecked(self, now_ms: u64) -> bool {
        self.enabled
            && now_ms >= self.enabled_at_ms
            && self.expires_at_ms.is_none_or(|expires| now_ms < expires)
    }
}

#[derive(PartialEq, Eq)]
pub struct QrHandoffToken {
    token: [u8; 24],
    item_hash: [u8; 32],
    issued_at_ms: u64,
    expires_at_ms: u64,
    consumed: bool,
}

impl fmt::Debug for QrHandoffToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("QrHandoffToken")
            .field("token", &"[redacted]")
            .field("item_hash", &"[redacted]")
            .field("issued_at_ms", &self.issued_at_ms)
            .field("expires_at_ms", &self.expires_at_ms)
            .field("consumed", &self.consumed)
            .finish()
    }
}

impl QrHandoffToken {
    pub fn issue(item_hash: [u8; 32], issued_at_ms: u64, ttl_ms: u64) -> Result<Self> {
        if all_zero(&item_hash) || ttl_ms == 0 || ttl_ms > MAX_QR_TOKEN_TTL_MS {
            return Err(SyncError::Invalid("invalid QR handoff lifetime".into()));
        }
        let mut token = [0_u8; 24];
        getrandom::fill(&mut token).map_err(|_| SyncError::Crypto)?;
        Ok(Self {
            token,
            item_hash,
            issued_at_ms,
            expires_at_ms: issued_at_ms
                .checked_add(ttl_ms)
                .ok_or_else(|| SyncError::Invalid("QR handoff expiry overflow".into()))?,
            consumed: false,
        })
    }

    pub fn payload(&self) -> String {
        format!(
            "vbuff://handoff/v1/{}?e={}",
            URL_SAFE_NO_PAD.encode(self.token),
            self.expires_at_ms
        )
    }

    pub const fn item_hash(&self) -> [u8; 32] {
        self.item_hash
    }

    pub const fn issued_at_ms(&self) -> u64 {
        self.issued_at_ms
    }

    pub const fn expires_at_ms(&self) -> u64 {
        self.expires_at_ms
    }

    pub fn consume(&mut self, presented_token: &str, now_ms: u64) -> bool {
        let accepted = !self.consumed
            && now_ms >= self.issued_at_ms
            && now_ms < self.expires_at_ms
            && URL_SAFE_NO_PAD
                .decode(presented_token)
                .ok()
                .is_some_and(|token| constant_time_eq(&token, &self.token));
        if accepted {
            self.consumed = true;
        }
        accepted
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}
