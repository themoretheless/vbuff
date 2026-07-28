use std::collections::BTreeMap;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use super::{invalid, valid_id};
use crate::{ActionPermissionRequest, PluginError, PluginManifest, Result};

const ACTION_BUNDLE_SCHEMA: u16 = 1;
const MAX_ACTIONS: usize = 256;
const SIGNATURE_DOMAIN: &[u8] = b"vbuff-action-bundle-v1\0";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionBundle {
    pub schema: u16,
    pub bundle_id: String,
    pub plugin_id: String,
    pub plugin_version: String,
    pub manifest_hash: [u8; 32],
    pub actions: BTreeMap<String, ActionPermissionRequest>,
}

impl ActionBundle {
    pub fn validate(&self, manifest: &PluginManifest) -> Result<()> {
        manifest.validate()?;
        if self.schema != ACTION_BUNDLE_SCHEMA
            || !valid_id(&self.bundle_id)
            || self.plugin_id != manifest.id
            || self.plugin_version != manifest.version
            || self.manifest_hash != manifest.hash()?
            || self.actions.is_empty()
            || self.actions.len() > MAX_ACTIONS
        {
            return invalid("action bundle identity or size is invalid");
        }
        for (action_id, request) in &self.actions {
            if action_id != &request.action_id {
                return invalid("action bundle key does not match action id");
            }
            request.validate(manifest)?;
        }
        Ok(())
    }

    pub fn canonical_bytes(&self, manifest: &PluginManifest) -> Result<Vec<u8>> {
        self.validate(manifest)?;
        serde_json::to_vec(self).map_err(|error| PluginError::Serialization(error.to_string()))
    }

    pub fn hash(&self, manifest: &PluginManifest) -> Result<[u8; 32]> {
        Ok(*blake3::hash(&self.canonical_bytes(manifest)?).as_bytes())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedActionBundle {
    pub bundle_hash: [u8; 32],
    pub signer_public_key: [u8; 32],
    pub signature: Vec<u8>,
}

impl SignedActionBundle {
    pub fn sign(
        bundle: &ActionBundle,
        manifest: &PluginManifest,
        key: &SigningKey,
    ) -> Result<Self> {
        let bundle_hash = bundle.hash(manifest)?;
        Ok(Self {
            bundle_hash,
            signer_public_key: key.verifying_key().to_bytes(),
            signature: key
                .sign(&signature_payload(&bundle_hash))
                .to_bytes()
                .to_vec(),
        })
    }

    pub fn verify(&self, bundle: &ActionBundle, manifest: &PluginManifest) -> Result<()> {
        if self.bundle_hash != bundle.hash(manifest)? {
            return Err(PluginError::InvalidSignature);
        }
        let key = VerifyingKey::from_bytes(&self.signer_public_key)
            .map_err(|_| PluginError::InvalidSignature)?;
        let signature =
            Signature::from_slice(&self.signature).map_err(|_| PluginError::InvalidSignature)?;
        key.verify(&signature_payload(&self.bundle_hash), &signature)
            .map_err(|_| PluginError::InvalidSignature)
    }
}

fn signature_payload(bundle_hash: &[u8; 32]) -> Vec<u8> {
    [SIGNATURE_DOMAIN, bundle_hash].concat()
}
