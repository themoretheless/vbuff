//! Human-readable, git-friendly, optionally signed snippet packs.

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::{PluginError, Result};

const MAX_SNIPPETS: usize = 10_000;
const MAX_PACK_BYTES: usize = 16 * 1024 * 1024;
const SIGNATURE_DOMAIN: &[u8] = b"vbuff-snippet-pack-v1\0";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnippetDefinition {
    pub trigger: String,
    pub expansion: String,
    #[serde(default)]
    pub description: Option<String>,
}

impl std::fmt::Debug for SnippetDefinition {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SnippetDefinition")
            .field("trigger", &self.trigger)
            .field("expansion", &"[redacted]")
            .field("description", &self.description)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnippetPack {
    pub schema: u16,
    pub id: String,
    pub version: String,
    pub snippets: BTreeMap<String, SnippetDefinition>,
}

impl std::fmt::Debug for SnippetPack {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SnippetPack")
            .field("schema", &self.schema)
            .field("id", &self.id)
            .field("version", &self.version)
            .field("snippet_count", &self.snippets.len())
            .finish()
    }
}

impl SnippetPack {
    pub fn validate(&self) -> Result<()> {
        if self.schema != 1 || !valid_id(&self.id) || !valid_version(&self.version) {
            return Err(PluginError::InvalidBundle(
                "snippet pack identity is invalid".into(),
            ));
        }
        if self.snippets.is_empty() || self.snippets.len() > MAX_SNIPPETS {
            return Err(PluginError::InvalidBundle(
                "snippet pack count is outside the supported range".into(),
            ));
        }
        let mut triggers = BTreeSet::new();
        let mut content_bytes = 0_usize;
        for (id, snippet) in &self.snippets {
            if !valid_id(id)
                || snippet.trigger.trim().is_empty()
                || snippet.trigger.len() > 128
                || snippet.trigger.chars().any(char::is_control)
                || snippet.expansion.len() > 1_048_576
                || snippet
                    .description
                    .as_ref()
                    .is_some_and(|text| text.len() > 512)
                || !triggers.insert(&snippet.trigger)
            {
                return Err(PluginError::InvalidBundle(
                    "snippet definition is invalid".into(),
                ));
            }
            content_bytes = content_bytes
                .saturating_add(id.len())
                .saturating_add(snippet.trigger.len())
                .saturating_add(snippet.expansion.len())
                .saturating_add(snippet.description.as_ref().map_or(0, String::len));
            if content_bytes > MAX_PACK_BYTES {
                return Err(PluginError::InvalidBundle(
                    "snippet pack is too large".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn canonical_toml(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let text = toml::to_string_pretty(self)
            .map_err(|error| PluginError::Serialization(error.to_string()))?;
        if text.len() > MAX_PACK_BYTES {
            return Err(PluginError::InvalidBundle(
                "snippet pack is too large".into(),
            ));
        }
        Ok(text.into_bytes())
    }

    pub fn from_toml(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_PACK_BYTES {
            return Err(PluginError::InvalidBundle(
                "snippet pack is too large".into(),
            ));
        }
        let text = std::str::from_utf8(bytes)
            .map_err(|_| PluginError::InvalidBundle("snippet pack is not UTF-8".into()))?;
        let pack: Self =
            toml::from_str(text).map_err(|error| PluginError::Serialization(error.to_string()))?;
        pack.validate()?;
        Ok(pack)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedSnippetPack {
    pub signer_public_key: [u8; 32],
    pub pack_hash: [u8; 32],
    pub signature: Vec<u8>,
}

impl SignedSnippetPack {
    pub fn sign(pack: &SnippetPack, key: &SigningKey) -> Result<Self> {
        let pack_hash = *blake3::hash(&pack.canonical_toml()?).as_bytes();
        Ok(Self {
            signer_public_key: key.verifying_key().to_bytes(),
            pack_hash,
            signature: key.sign(&signature_payload(&pack_hash)).to_bytes().to_vec(),
        })
    }

    pub fn verify(&self, pack: &SnippetPack) -> Result<()> {
        let expected = *blake3::hash(&pack.canonical_toml()?).as_bytes();
        if expected != self.pack_hash {
            return Err(PluginError::InvalidSignature);
        }
        let key = VerifyingKey::from_bytes(&self.signer_public_key)
            .map_err(|_| PluginError::InvalidSignature)?;
        let signature =
            Signature::from_slice(&self.signature).map_err(|_| PluginError::InvalidSignature)?;
        key.verify(&signature_payload(&self.pack_hash), &signature)
            .map_err(|_| PluginError::InvalidSignature)
    }
}

fn signature_payload(pack_hash: &[u8; 32]) -> Vec<u8> {
    [SIGNATURE_DOMAIN, pack_hash].concat()
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
}

fn valid_version(value: &str) -> bool {
    let mut parts = value.split('.');
    let valid = (0..3).all(|_| {
        parts
            .next()
            .is_some_and(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    });
    valid && parts.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_pack() -> SnippetPack {
        SnippetPack {
            schema: 1,
            id: "dev.vbuff.git-basics".into(),
            version: "1.0.0".into(),
            snippets: BTreeMap::from([(
                "status".into(),
                SnippetDefinition {
                    trigger: ";gs".into(),
                    expansion: "git status --short".into(),
                    description: Some("Concise status".into()),
                },
            )]),
        }
    }

    #[test]
    fn pack_is_stable_plain_toml_and_signature_detects_edits() {
        let pack = fixture_pack();
        let bytes = pack.canonical_toml().unwrap();
        assert_eq!(
            SnippetPack::from_toml(&bytes)
                .unwrap()
                .canonical_toml()
                .unwrap(),
            bytes
        );
        let key = SigningKey::from_bytes(&[4; 32]);
        let signed = SignedSnippetPack::sign(&pack, &key).unwrap();
        signed.verify(&pack).unwrap();
        let mut changed = pack;
        changed
            .snippets
            .get_mut("status")
            .unwrap()
            .expansion
            .push_str(" --branch");
        assert_eq!(signed.verify(&changed), Err(PluginError::InvalidSignature));

        let mut duplicate = fixture_pack();
        duplicate.snippets.insert(
            "duplicate".into(),
            SnippetDefinition {
                trigger: ";gs".into(),
                expansion: "git status".into(),
                description: None,
            },
        );
        assert!(duplicate.validate().is_err());
    }
}
