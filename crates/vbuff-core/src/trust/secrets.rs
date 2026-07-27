use std::fmt;
use std::time::Duration;

use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use serde::Serialize;
use thiserror::Error;
pub use vbuff_types::SensitivityReason;

use crate::secret::SecretKind;

const UPDATE_DOMAIN: &[u8] = b"vbuff-detector-update-v1\0";
const MAX_DETECTORS: usize = 512;
const MAX_DETECTOR_ID_BYTES: usize = 128;
const MAX_UPDATE_LIFETIME_MS: u64 = 31 * 24 * 60 * 60 * 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretMask {
    Full,
    LastFour,
    Grouped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SecretHandling {
    pub ttl: Duration,
    pub mask: SecretMask,
    pub memory_only: bool,
    pub sync_allowed: bool,
}

pub const fn handling_for_secret(kind: SecretKind) -> SecretHandling {
    match kind {
        SecretKind::PrivateKey => handling(30, SecretMask::Full, true),
        SecretKind::CloudCredential | SecretKind::AccessToken => {
            handling(5 * 60, SecretMask::LastFour, false)
        }
        SecretKind::JsonWebToken => handling(2 * 60, SecretMask::Full, false),
        SecretKind::PaymentCard => handling(10 * 60, SecretMask::LastFour, false),
        SecretKind::OneTimePassword => handling(60, SecretMask::Grouped, true),
        SecretKind::RecoveryCode => handling(30, SecretMask::Full, true),
        SecretKind::HighEntropy => handling(2 * 60, SecretMask::Full, false),
    }
}

const fn handling(seconds: u64, mask: SecretMask, memory_only: bool) -> SecretHandling {
    SecretHandling {
        ttl: Duration::from_secs(seconds),
        mask,
        memory_only,
        sync_allowed: false,
    }
}

pub const fn sensitivity_watermark(reason: SensitivityReason) -> &'static str {
    reason.watermark()
}

pub const fn sensitivity_reason_for_secret(kind: SecretKind) -> SensitivityReason {
    match kind {
        SecretKind::PrivateKey => SensitivityReason::PrivateKey,
        SecretKind::CloudCredential => SensitivityReason::CloudCredential,
        SecretKind::AccessToken => SensitivityReason::AccessToken,
        SecretKind::JsonWebToken => SensitivityReason::JsonWebToken,
        SecretKind::PaymentCard => SensitivityReason::PaymentCard,
        SecretKind::OneTimePassword => SensitivityReason::OneTimePassword,
        SecretKind::RecoveryCode => SensitivityReason::RecoveryCode,
        SecretKind::HighEntropy => SensitivityReason::PossibleSecret,
    }
}

#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct SignedDetectorUpdate {
    version: u64,
    issued_at_ms: u64,
    expires_at_ms: u64,
    detector_ids: Vec<String>,
    signature: Vec<u8>,
}

impl fmt::Debug for SignedDetectorUpdate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignedDetectorUpdate")
            .field("version", &self.version)
            .field("issued_at_ms", &self.issued_at_ms)
            .field("expires_at_ms", &self.expires_at_ms)
            .field("detector_count", &self.detector_ids.len())
            .field("signature", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum DetectorUpdateError {
    #[error("detector update is invalid")]
    Invalid,
    #[error("detector update signature is invalid")]
    InvalidSignature,
    #[error("detector update is stale")]
    Stale,
    #[error("detector update is expired")]
    Expired,
}

impl SignedDetectorUpdate {
    pub fn issue(
        version: u64,
        issued_at_ms: u64,
        expires_at_ms: u64,
        detector_ids: impl IntoIterator<Item = String>,
        signing_key: &SigningKey,
    ) -> Result<Self, DetectorUpdateError> {
        let mut detector_ids = detector_ids.into_iter().collect::<Vec<_>>();
        detector_ids.sort();
        detector_ids.dedup();
        validate_update_fields(version, issued_at_ms, expires_at_ms, &detector_ids)?;
        let signature = signing_key
            .sign(&update_signing_bytes(
                version,
                issued_at_ms,
                expires_at_ms,
                &detector_ids,
            ))
            .to_bytes()
            .to_vec();
        Ok(Self {
            version,
            issued_at_ms,
            expires_at_ms,
            detector_ids,
            signature,
        })
    }

    pub fn verify(
        &self,
        current_version: u64,
        now_ms: u64,
        verifying_key: &VerifyingKey,
    ) -> Result<&[String], DetectorUpdateError> {
        validate_update_fields(
            self.version,
            self.issued_at_ms,
            self.expires_at_ms,
            &self.detector_ids,
        )?;
        if self.version <= current_version {
            return Err(DetectorUpdateError::Stale);
        }
        if now_ms < self.issued_at_ms || now_ms > self.expires_at_ms {
            return Err(DetectorUpdateError::Expired);
        }
        let signature = Signature::from_slice(&self.signature)
            .map_err(|_| DetectorUpdateError::InvalidSignature)?;
        verifying_key
            .verify(
                &update_signing_bytes(
                    self.version,
                    self.issued_at_ms,
                    self.expires_at_ms,
                    &self.detector_ids,
                ),
                &signature,
            )
            .map_err(|_| DetectorUpdateError::InvalidSignature)?;
        Ok(&self.detector_ids)
    }

    pub const fn version(&self) -> u64 {
        self.version
    }
}

fn validate_update_fields(
    version: u64,
    issued_at_ms: u64,
    expires_at_ms: u64,
    detector_ids: &[String],
) -> Result<(), DetectorUpdateError> {
    if version == 0
        || detector_ids.is_empty()
        || detector_ids.len() > MAX_DETECTORS
        || expires_at_ms <= issued_at_ms
        || expires_at_ms.saturating_sub(issued_at_ms) > MAX_UPDATE_LIFETIME_MS
        || detector_ids.windows(2).any(|pair| pair[0] >= pair[1])
        || detector_ids.iter().any(|id| !valid_detector_id(id))
    {
        return Err(DetectorUpdateError::Invalid);
    }
    Ok(())
}

fn valid_detector_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_DETECTOR_ID_BYTES
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn update_signing_bytes(
    version: u64,
    issued_at_ms: u64,
    expires_at_ms: u64,
    detector_ids: &[String],
) -> Vec<u8> {
    let identifiers_bytes = detector_ids
        .iter()
        .map(String::len)
        .fold(0usize, usize::saturating_add);
    let mut bytes = Vec::with_capacity(
        UPDATE_DOMAIN.len() + 8 * 3 + 4 + detector_ids.len() * 4 + identifiers_bytes,
    );
    bytes.extend_from_slice(UPDATE_DOMAIN);
    bytes.extend_from_slice(&version.to_be_bytes());
    bytes.extend_from_slice(&issued_at_ms.to_be_bytes());
    bytes.extend_from_slice(&expires_at_ms.to_be_bytes());
    bytes.extend_from_slice(&(detector_ids.len() as u32).to_be_bytes());
    for id in detector_ids {
        bytes.extend_from_slice(&(id.len() as u32).to_be_bytes());
        bytes.extend_from_slice(id.as_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_types_receive_distinct_fail_closed_handling() {
        let otp = handling_for_secret(SecretKind::OneTimePassword);
        let card = handling_for_secret(SecretKind::PaymentCard);
        assert!(otp.ttl < card.ttl);
        assert_eq!(otp.mask, SecretMask::Grouped);
        assert_eq!(card.mask, SecretMask::LastFour);
        assert!(otp.memory_only);
        assert!(!card.sync_allowed);
        assert_eq!(
            sensitivity_watermark(SensitivityReason::Entropy),
            "Masked: high entropy"
        );
    }

    #[test]
    fn detector_updates_are_bounded_versioned_and_authenticated() {
        let key = SigningKey::from_bytes(&[7_u8; 32]);
        let update = SignedDetectorUpdate::issue(
            3,
            1_000,
            2_000,
            ["password-manager.example".into(), "token.github".into()],
            &key,
        )
        .unwrap();
        assert_eq!(
            update.verify(2, 1_500, &key.verifying_key()).unwrap().len(),
            2
        );
        assert_eq!(
            update.verify(3, 1_500, &key.verifying_key()),
            Err(DetectorUpdateError::Stale)
        );
        assert_eq!(
            update.verify(2, 2_001, &key.verifying_key()),
            Err(DetectorUpdateError::Expired)
        );
        let wrong_key = SigningKey::from_bytes(&[8; 32]);
        assert_eq!(
            update.verify(2, 1_500, &wrong_key.verifying_key()),
            Err(DetectorUpdateError::InvalidSignature)
        );
        assert!(!format!("{update:?}").contains("password-manager"));
    }
}
