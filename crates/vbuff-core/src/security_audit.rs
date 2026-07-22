//! Tamper-evident, content-free local security audit records.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use zeroize::Zeroize;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecurityEvent {
    VaultUnlocked,
    VaultLocked,
    SecretDetected,
    SensitiveClawback,
    ExportStarted,
    ExportFinished,
    PluginCapabilityGranted,
    IntegrityFailure,
}

impl SecurityEvent {
    const fn code(self) -> u8 {
        match self {
            Self::VaultUnlocked => 1,
            Self::VaultLocked => 2,
            Self::SecretDetected => 3,
            Self::SensitiveClawback => 4,
            Self::ExportStarted => 5,
            Self::ExportFinished => 6,
            Self::PluginCapabilityGranted => 7,
            Self::IntegrityFailure => 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecurityAuditEntry {
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub event: SecurityEvent,
    pub previous_mac: [u8; 32],
    pub mac: [u8; 32],
}

pub struct SecurityAuditChain {
    key: [u8; 32],
    next_sequence: u64,
    previous_mac: [u8; 32],
}

impl Drop for SecurityAuditChain {
    fn drop(&mut self) {
        self.key.zeroize();
        self.previous_mac.zeroize();
    }
}

impl SecurityAuditChain {
    pub fn new(key: [u8; 32]) -> Self {
        Self {
            key,
            next_sequence: 0,
            previous_mac: [0; 32],
        }
    }

    pub fn append(&mut self, timestamp_ms: u64, event: SecurityEvent) -> SecurityAuditEntry {
        let sequence = self.next_sequence;
        let previous_mac = self.previous_mac;
        let mac = calculate_mac(&self.key, sequence, timestamp_ms, event, &previous_mac);
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.previous_mac = mac;
        SecurityAuditEntry {
            sequence,
            timestamp_ms,
            event,
            previous_mac,
            mac,
        }
    }

    pub fn verify(key: &[u8; 32], entries: &[SecurityAuditEntry]) -> bool {
        let mut expected_sequence = 0_u64;
        let mut previous_mac = [0_u8; 32];
        entries.iter().all(|entry| {
            let valid = entry.sequence == expected_sequence
                && entry.previous_mac == previous_mac
                && calculate_mac(
                    key,
                    entry.sequence,
                    entry.timestamp_ms,
                    entry.event,
                    &entry.previous_mac,
                ) == entry.mac;
            expected_sequence = expected_sequence.saturating_add(1);
            previous_mac = entry.mac;
            valid
        })
    }
}

fn calculate_mac(
    key: &[u8; 32],
    sequence: u64,
    timestamp_ms: u64,
    event: SecurityEvent,
    previous_mac: &[u8; 32],
) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(b"vbuff-security-audit-v1");
    mac.update(&sequence.to_be_bytes());
    mac.update(&timestamp_ms.to_be_bytes());
    mac.update(&[event.code()]);
    mac.update(previous_mac);
    mac.finalize().into_bytes().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_verifies_and_detects_tampering() {
        let key = [7; 32];
        let mut chain = SecurityAuditChain::new(key);
        let mut entries = vec![
            chain.append(10, SecurityEvent::VaultUnlocked),
            chain.append(12, SecurityEvent::SecretDetected),
        ];
        assert!(SecurityAuditChain::verify(&key, &entries));
        entries[0].timestamp_ms = 11;
        assert!(!SecurityAuditChain::verify(&key, &entries));
    }
}
