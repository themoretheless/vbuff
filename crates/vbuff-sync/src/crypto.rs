//! AEAD key wrapping, scheduled re-wrap, and sealed-sender envelopes.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};
use zeroize::Zeroizing;

use crate::{Result, SyncError};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedKey {
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
}

pub fn wrap_key(kek: &[u8; 32], key: &[u8; 32], aad: &[u8]) -> Result<WrappedKey> {
    let nonce = random_array()?;
    let cipher_nonce = XNonce::from(nonce);
    let cipher = XChaCha20Poly1305::new(kek.into());
    let ciphertext = cipher
        .encrypt(&cipher_nonce, Payload { msg: key, aad })
        .map_err(|_| SyncError::Crypto)?;
    Ok(WrappedKey { nonce, ciphertext })
}

pub fn unwrap_key(kek: &[u8; 32], wrapped: &WrappedKey, aad: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    let cipher = XChaCha20Poly1305::new(kek.into());
    let nonce = XNonce::from(wrapped.nonce);
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &wrapped.ciphertext,
                    aad,
                },
            )
            .map_err(|_| SyncError::Crypto)?,
    );
    let key = plaintext
        .as_slice()
        .try_into()
        .map_err(|_| SyncError::Invalid("wrapped key has wrong length".into()))?;
    Ok(Zeroizing::new(key))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedContentKey {
    pub record_id: String,
    pub root_epoch: u64,
    pub wrapped: WrappedKey,
}

pub fn rewrap_content_keys(
    records: &[WrappedContentKey],
    old_kek: &[u8; 32],
    new_kek: &[u8; 32],
    new_epoch: u64,
) -> Result<Vec<WrappedContentKey>> {
    records
        .iter()
        .map(|record| {
            let content_key = unwrap_key(old_kek, &record.wrapped, record.record_id.as_bytes())?;
            Ok(WrappedContentKey {
                record_id: record.record_id.clone(),
                root_epoch: new_epoch,
                wrapped: wrap_key(new_kek, &content_key, record.record_id.as_bytes())?,
            })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedEnvelope {
    pub ephemeral_public_key: [u8; 32],
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
}

/// Relay-visible wrapper carries only an epoch-rotating opaque route and the
/// sender-anonymous sealed payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayEnvelope {
    pub epoch: u64,
    pub routing_tag: [u8; 16],
    pub sealed: SealedEnvelope,
}

pub fn seal_for_relay(
    routing_secret: &[u8; 32],
    epoch: u64,
    recipient_public_key: &[u8; 32],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<RelayEnvelope> {
    let mut hasher = blake3::Hasher::new_keyed(routing_secret);
    hasher.update(b"vbuff-relay-route-v1");
    hasher.update(&epoch.to_le_bytes());
    let digest = hasher.finalize();
    let mut routing_tag = [0_u8; 16];
    routing_tag.copy_from_slice(&digest.as_bytes()[..16]);
    Ok(RelayEnvelope {
        epoch,
        routing_tag,
        sealed: seal_to(recipient_public_key, plaintext, aad)?,
    })
}

pub fn seal_to(
    recipient_public_key: &[u8; 32],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<SealedEnvelope> {
    let recipient = PublicKey::from(*recipient_public_key);
    let ephemeral_secret = EphemeralSecret::random();
    let ephemeral_public = PublicKey::from(&ephemeral_secret);
    let shared = ephemeral_secret.diffie_hellman(&recipient);
    if !shared.was_contributory() {
        return Err(SyncError::Crypto);
    }
    let key = derive_sealed_key(
        shared.as_bytes(),
        ephemeral_public.as_bytes(),
        recipient.as_bytes(),
    )?;
    let nonce = random_array()?;
    let cipher_nonce = XNonce::from(nonce);
    let cipher = XChaCha20Poly1305::new((&*key).into());
    let ciphertext = cipher
        .encrypt(
            &cipher_nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| SyncError::Crypto)?;
    Ok(SealedEnvelope {
        ephemeral_public_key: ephemeral_public.to_bytes(),
        nonce,
        ciphertext,
    })
}

pub fn open_sealed(
    recipient_secret: &StaticSecret,
    envelope: &SealedEnvelope,
    aad: &[u8],
) -> Result<Vec<u8>> {
    let recipient_public = PublicKey::from(recipient_secret);
    let ephemeral_public = PublicKey::from(envelope.ephemeral_public_key);
    let shared = recipient_secret.diffie_hellman(&ephemeral_public);
    if !shared.was_contributory() {
        return Err(SyncError::Crypto);
    }
    let key = derive_sealed_key(
        shared.as_bytes(),
        ephemeral_public.as_bytes(),
        recipient_public.as_bytes(),
    )?;
    let nonce = XNonce::from(envelope.nonce);
    XChaCha20Poly1305::new((&*key).into())
        .decrypt(
            &nonce,
            Payload {
                msg: &envelope.ciphertext,
                aad,
            },
        )
        .map_err(|_| SyncError::Crypto)
}

fn derive_sealed_key(
    shared: &[u8; 32],
    ephemeral_public: &[u8; 32],
    recipient_public: &[u8; 32],
) -> Result<Zeroizing<[u8; 32]>> {
    let mut salt = [0_u8; 64];
    salt[..32].copy_from_slice(ephemeral_public);
    salt[32..].copy_from_slice(recipient_public);
    let mut output = [0_u8; 32];
    Hkdf::<Sha256>::new(Some(&salt), shared)
        .expand(b"vbuff-sealed-sender-v1", &mut output)
        .map_err(|_| SyncError::Crypto)?;
    Ok(Zeroizing::new(output))
}

fn random_array<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0_u8; N];
    getrandom::fill(&mut bytes).map_err(|_| SyncError::Crypto)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrap_changes_root_without_reencrypting_payload_key() {
        let old = [1_u8; 32];
        let new = [2_u8; 32];
        let content = [3_u8; 32];
        let records = vec![WrappedContentKey {
            record_id: "clip-1".into(),
            root_epoch: 1,
            wrapped: wrap_key(&old, &content, b"clip-1").unwrap(),
        }];
        let rotated = rewrap_content_keys(&records, &old, &new, 2).unwrap();
        assert_eq!(rotated[0].root_epoch, 2);
        assert_eq!(
            *unwrap_key(&new, &rotated[0].wrapped, b"clip-1").unwrap(),
            content
        );
        assert!(unwrap_key(&old, &rotated[0].wrapped, b"clip-1").is_err());
    }

    #[test]
    fn sealed_sender_roundtrips_without_sender_identity() {
        let recipient_secret = StaticSecret::from([7_u8; 32]);
        let recipient_public = PublicKey::from(&recipient_secret).to_bytes();
        let envelope = seal_to(&recipient_public, b"private clip", b"epoch-4").unwrap();
        assert_eq!(
            open_sealed(&recipient_secret, &envelope, b"epoch-4").unwrap(),
            b"private clip"
        );
        assert!(open_sealed(&recipient_secret, &envelope, b"epoch-5").is_err());
        assert!(seal_to(&[0; 32], b"private clip", b"epoch-4").is_err());
    }

    #[test]
    fn relay_routing_tag_rotates_with_the_epoch() {
        let recipient_secret = StaticSecret::from([13_u8; 32]);
        let recipient_public = PublicKey::from(&recipient_secret).to_bytes();
        let first =
            seal_for_relay(&[21; 32], 4, &recipient_public, b"private clip", b"epoch-4").unwrap();
        let next =
            seal_for_relay(&[21; 32], 5, &recipient_public, b"private clip", b"epoch-5").unwrap();
        assert_ne!(first.routing_tag, next.routing_tag);
        assert_eq!(
            open_sealed(&recipient_secret, &first.sealed, b"epoch-4").unwrap(),
            b"private clip"
        );
    }
}
