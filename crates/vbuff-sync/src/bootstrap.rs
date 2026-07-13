//! Offline recovery phrase and encrypted QR/bootstrap snapshots.

use std::io::{Cursor, Read};

use bip39::Mnemonic;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::{Result, SyncError};

const BOOTSTRAP_AAD: &[u8] = b"vbuff-bootstrap-v1";
const MAX_BOOTSTRAP_BYTES: usize = 512 * 1024;
const MAX_BOOTSTRAP_CIPHERTEXT: usize = MAX_BOOTSTRAP_BYTES + 64 * 1024;

pub fn generate_recovery_phrase() -> Result<String> {
    let mut entropy = Zeroizing::new([0_u8; 32]);
    getrandom::fill(&mut *entropy).map_err(|_| SyncError::Crypto)?;
    Ok(Mnemonic::from_entropy(&*entropy)
        .map_err(|error| SyncError::Invalid(error.to_string()))?
        .to_string())
}

pub fn recovery_root(phrase: &str) -> Result<[u8; 32]> {
    let mnemonic: Mnemonic = phrase
        .parse()
        .map_err(|error: bip39::Error| SyncError::Invalid(error.to_string()))?;
    let seed = Zeroizing::new(mnemonic.to_seed("vbuff-recovery-v1"));
    let mut root = [0_u8; 32];
    Hkdf::<Sha256>::new(None, &*seed)
        .expand(b"vbuff-group-membership-root-v1", &mut root)
        .map_err(|_| SyncError::Crypto)?;
    Ok(root)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedBootstrap {
    pub version: u8,
    pub salt: [u8; 16],
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
}

pub fn encrypt_snapshot<T: Serialize>(root: &[u8; 32], snapshot: &T) -> Result<EncryptedBootstrap> {
    let serialized = Zeroizing::new(serde_json::to_vec(snapshot)?);
    if serialized.len() > MAX_BOOTSTRAP_BYTES {
        return Err(SyncError::Invalid(
            "bootstrap snapshot exceeds size limit".into(),
        ));
    }
    let compressed = Zeroizing::new(zstd::stream::encode_all(
        Cursor::new(serialized.as_slice()),
        3,
    )?);
    let mut salt = [0_u8; 16];
    let mut nonce = [0_u8; 24];
    getrandom::fill(&mut salt).map_err(|_| SyncError::Crypto)?;
    getrandom::fill(&mut nonce).map_err(|_| SyncError::Crypto)?;
    let key = bootstrap_key(root, &salt)?;
    let cipher_nonce = XNonce::from(nonce);
    let ciphertext = XChaCha20Poly1305::new((&*key).into())
        .encrypt(
            &cipher_nonce,
            Payload {
                msg: compressed.as_slice(),
                aad: BOOTSTRAP_AAD,
            },
        )
        .map_err(|_| SyncError::Crypto)?;
    Ok(EncryptedBootstrap {
        version: 1,
        salt,
        nonce,
        ciphertext,
    })
}

pub fn decrypt_snapshot<T: DeserializeOwned>(
    root: &[u8; 32],
    bootstrap: &EncryptedBootstrap,
) -> Result<T> {
    if bootstrap.version != 1 {
        return Err(SyncError::Invalid("unsupported bootstrap version".into()));
    }
    if bootstrap.ciphertext.len() > MAX_BOOTSTRAP_CIPHERTEXT {
        return Err(SyncError::Invalid(
            "bootstrap ciphertext exceeds size limit".into(),
        ));
    }
    let key = bootstrap_key(root, &bootstrap.salt)?;
    let nonce = XNonce::from(bootstrap.nonce);
    let compressed = Zeroizing::new(
        XChaCha20Poly1305::new((&*key).into())
            .decrypt(
                &nonce,
                Payload {
                    msg: &bootstrap.ciphertext,
                    aad: BOOTSTRAP_AAD,
                },
            )
            .map_err(|_| SyncError::Crypto)?,
    );
    let decoder = zstd::stream::read::Decoder::new(Cursor::new(compressed.as_slice()))?;
    let mut serialized = Zeroizing::new(Vec::new());
    decoder
        .take((MAX_BOOTSTRAP_BYTES + 1) as u64)
        .read_to_end(&mut serialized)?;
    if serialized.len() > MAX_BOOTSTRAP_BYTES {
        return Err(SyncError::Invalid(
            "decompressed bootstrap exceeds size limit".into(),
        ));
    }
    Ok(serde_json::from_slice(serialized.as_slice())?)
}

fn bootstrap_key(root: &[u8; 32], salt: &[u8; 16]) -> Result<Zeroizing<[u8; 32]>> {
    let mut key = [0_u8; 32];
    Hkdf::<Sha256>::new(Some(salt), root)
        .expand(BOOTSTRAP_AAD, &mut key)
        .map_err(|_| SyncError::Crypto)?;
    Ok(Zeroizing::new(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct SeedSnapshot {
        pinned: Vec<String>,
        snippets: Vec<String>,
    }

    #[test]
    fn recovery_phrase_has_24_words_and_is_deterministic() {
        let phrase = generate_recovery_phrase().unwrap();
        assert_eq!(phrase.split_whitespace().count(), 24);
        assert_eq!(
            recovery_root(&phrase).unwrap(),
            recovery_root(&phrase).unwrap()
        );
    }

    #[test]
    fn encrypted_snapshot_roundtrips_and_rejects_wrong_root() {
        let snapshot = SeedSnapshot {
            pinned: vec!["clip-a".into()],
            snippets: vec!["sig".into()],
        };
        let encrypted = encrypt_snapshot(&[4; 32], &snapshot).unwrap();
        assert_eq!(
            decrypt_snapshot::<SeedSnapshot>(&[4; 32], &encrypted).unwrap(),
            snapshot
        );
        assert!(decrypt_snapshot::<SeedSnapshot>(&[5; 32], &encrypted).is_err());
    }

    #[test]
    fn decompression_is_bounded_before_allocating_the_full_output() {
        let root = [6_u8; 32];
        let oversized = vec![b'x'; MAX_BOOTSTRAP_BYTES + 1];
        let compressed = zstd::stream::encode_all(Cursor::new(oversized), 3).unwrap();
        let salt = [1_u8; 16];
        let nonce = [2_u8; 24];
        let key = bootstrap_key(&root, &salt).unwrap();
        let ciphertext = XChaCha20Poly1305::new((&*key).into())
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &compressed,
                    aad: BOOTSTRAP_AAD,
                },
            )
            .unwrap();
        let bootstrap = EncryptedBootstrap {
            version: 1,
            salt,
            nonce,
            ciphertext,
        };

        assert!(decrypt_snapshot::<serde_json::Value>(&root, &bootstrap).is_err());
    }
}
