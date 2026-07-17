//! Transport-agnostic encrypted history export and verified restore.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use vbuff_types::Clip;

use crate::{Result, SyncError};

const DOMAIN: &[u8] = b"vbuff-portable-vault-v1";
const MAGIC: [u8; 8] = *b"VBUFFVLT";
const HEADER_BYTES: usize = MAGIC.len() + 2 + 8 + 8 + 32 + 24 + 8;
const MAX_PLAINTEXT_BYTES: usize = 512 * 1024 * 1024;
const MAX_CIPHERTEXT_BYTES: usize = MAX_PLAINTEXT_BYTES + 16;
const MAX_SERIALIZED_BYTES: usize = HEADER_BYTES + MAX_CIPHERTEXT_BYTES;
const MAX_RECORDS: usize = 1_000_000;

#[derive(Clone, PartialEq, Eq)]
pub struct PortableVault {
    pub schema: u16,
    pub created_at_ms: u64,
    pub record_count: u64,
    pub plaintext_hash: [u8; 32],
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
}

impl std::fmt::Debug for PortableVault {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PortableVault")
            .field("schema", &self.schema)
            .field("created_at_ms", &self.created_at_ms)
            .field("record_count", &self.record_count)
            .field("ciphertext_bytes", &self.ciphertext.len())
            .finish()
    }
}

impl PortableVault {
    pub fn seal(key: &[u8; 32], clips: &[Clip], created_at_ms: u64) -> Result<Self> {
        if clips.len() > MAX_RECORDS {
            return Err(SyncError::Invalid("vault has too many records".into()));
        }
        let plaintext = serde_json::to_vec(clips)?;
        if plaintext.len() > MAX_PLAINTEXT_BYTES {
            return Err(SyncError::Invalid("vault payload is too large".into()));
        }
        let plaintext_hash = *blake3::hash(&plaintext).as_bytes();
        let record_count = clips.len() as u64;
        let nonce = random_nonce()?;
        let aad = aad(created_at_ms, record_count, &plaintext_hash);
        let ciphertext = XChaCha20Poly1305::new(key.into())
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| SyncError::Crypto)?;
        Ok(Self {
            schema: 1,
            created_at_ms,
            record_count,
            plaintext_hash,
            nonce,
            ciphertext,
        })
    }

    pub fn restore(&self, key: &[u8; 32]) -> Result<Vec<Clip>> {
        self.validate_shape()?;
        let aad = aad(self.created_at_ms, self.record_count, &self.plaintext_hash);
        let plaintext = XChaCha20Poly1305::new(key.into())
            .decrypt(
                &XNonce::from(self.nonce),
                Payload {
                    msg: &self.ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| SyncError::Crypto)?;
        if plaintext.len() > MAX_PLAINTEXT_BYTES
            || *blake3::hash(&plaintext).as_bytes() != self.plaintext_hash
        {
            return Err(SyncError::Invalid("vault integrity check failed".into()));
        }
        let clips: Vec<Clip> = serde_json::from_slice(&plaintext)?;
        if clips.len() as u64 != self.record_count {
            return Err(SyncError::Invalid("vault record count mismatch".into()));
        }
        Ok(clips)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.validate_shape()?;
        let mut bytes = Vec::with_capacity(HEADER_BYTES + self.ciphertext.len());
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&self.schema.to_be_bytes());
        bytes.extend_from_slice(&self.created_at_ms.to_be_bytes());
        bytes.extend_from_slice(&self.record_count.to_be_bytes());
        bytes.extend_from_slice(&self.plaintext_hash);
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&(self.ciphertext.len() as u64).to_be_bytes());
        bytes.extend_from_slice(&self.ciphertext);
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < HEADER_BYTES || bytes.len() > MAX_SERIALIZED_BYTES {
            return Err(SyncError::Invalid("vault container size is invalid".into()));
        }
        let mut cursor = 0;
        if take::<8>(bytes, &mut cursor)? != MAGIC {
            return Err(SyncError::Invalid("invalid portable vault magic".into()));
        }
        let schema = u16::from_be_bytes(take(bytes, &mut cursor)?);
        let created_at_ms = u64::from_be_bytes(take(bytes, &mut cursor)?);
        let record_count = u64::from_be_bytes(take(bytes, &mut cursor)?);
        let plaintext_hash = take(bytes, &mut cursor)?;
        let nonce = take(bytes, &mut cursor)?;
        let ciphertext_len_u64 = u64::from_be_bytes(take(bytes, &mut cursor)?);
        let ciphertext_len = usize::try_from(ciphertext_len_u64)
            .map_err(|_| SyncError::Invalid("invalid vault ciphertext length".into()))?;
        let expected_len = cursor
            .checked_add(ciphertext_len)
            .ok_or_else(|| SyncError::Invalid("invalid vault ciphertext length".into()))?;
        if expected_len != bytes.len() {
            return Err(SyncError::Invalid("invalid vault ciphertext length".into()));
        }
        let vault = Self {
            schema,
            created_at_ms,
            record_count,
            plaintext_hash,
            nonce,
            ciphertext: bytes[cursor..].to_vec(),
        };
        vault.validate_shape()?;
        Ok(vault)
    }

    fn validate_shape(&self) -> Result<()> {
        if self.schema != 1
            || self.record_count > MAX_RECORDS as u64
            || !(16..=MAX_CIPHERTEXT_BYTES).contains(&self.ciphertext.len())
        {
            return Err(SyncError::Invalid("invalid portable vault header".into()));
        }
        Ok(())
    }
}

fn take<const N: usize>(bytes: &[u8], cursor: &mut usize) -> Result<[u8; N]> {
    let end = cursor
        .checked_add(N)
        .ok_or_else(|| SyncError::Invalid("truncated portable vault".into()))?;
    let value = bytes
        .get(*cursor..end)
        .ok_or_else(|| SyncError::Invalid("truncated portable vault".into()))?;
    *cursor = end;
    value
        .try_into()
        .map_err(|_| SyncError::Invalid("truncated portable vault".into()))
}

fn aad(created_at_ms: u64, record_count: u64, hash: &[u8; 32]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(DOMAIN.len() + 48);
    aad.extend_from_slice(DOMAIN);
    aad.extend_from_slice(&created_at_ms.to_be_bytes());
    aad.extend_from_slice(&record_count.to_be_bytes());
    aad.extend_from_slice(hash);
    aad
}

fn random_nonce() -> Result<[u8; 24]> {
    let mut nonce = [0_u8; 24];
    getrandom::fill(&mut nonce).map_err(|_| SyncError::Crypto)?;
    Ok(nonce)
}

#[cfg(test)]
mod tests {
    use vbuff_types::{ClipId, ClipMeta, ContentKind, Flavor};

    use super::*;

    fn clip() -> Clip {
        let flavors = vec![Flavor::inline("text/plain", b"portable secret".to_vec())];
        Clip {
            id: ClipId::new(),
            content_hash: vbuff_core::content_hash_from_flavors(&flavors),
            meta: ClipMeta::now(ContentKind::Text, 15, None),
            flavors,
            pinned: true,
            favorite: false,
        }
    }

    #[test]
    fn portable_vault_roundtrips_byte_identical_records_and_rejects_tampering() {
        let clips = vec![clip()];
        let vault = PortableVault::seal(&[7; 32], &clips, 123).unwrap();
        assert_eq!(vault.restore(&[7; 32]).unwrap(), clips);
        let encoded = vault.to_bytes().unwrap();
        let decoded = PortableVault::from_bytes(&encoded).unwrap();
        assert_eq!(decoded, vault);
        assert_eq!(decoded.restore(&[7; 32]).unwrap(), clips);
        assert!(!format!("{vault:?}").contains("portable secret"));
        let mut tampered = vault.clone();
        tampered.ciphertext[0] ^= 1;
        assert!(tampered.restore(&[7; 32]).is_err());
        assert!(tampered.restore(&[8; 32]).is_err());

        let mut bad_magic = encoded.clone();
        bad_magic[0] ^= 1;
        assert!(PortableVault::from_bytes(&bad_magic).is_err());
        assert!(PortableVault::from_bytes(&encoded[..encoded.len() - 1]).is_err());
        let mut trailing = encoded;
        trailing.push(0);
        assert!(PortableVault::from_bytes(&trailing).is_err());
    }
}
