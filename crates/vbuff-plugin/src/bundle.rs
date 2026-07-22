use std::collections::BTreeMap;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::manifest::validate_relative_path;
use crate::{PluginError, PluginManifest, Result};

const MAX_EXECUTABLE_BYTES: usize = 64 * 1024 * 1024;
const MAX_ASSETS: usize = 1_024;
const MAX_ASSET_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub struct PluginBundle {
    pub manifest: PluginManifest,
    pub executable: Vec<u8>,
    pub assets: BTreeMap<String, Vec<u8>>,
}

impl std::fmt::Debug for PluginBundle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginBundle")
            .field("manifest", &self.manifest)
            .field("executable_bytes", &self.executable.len())
            .field("asset_count", &self.assets.len())
            .finish()
    }
}

impl PluginBundle {
    pub fn reproducible_bytes(&self) -> Result<Vec<u8>> {
        self.manifest.validate()?;
        if self.executable.is_empty() || self.executable.len() > MAX_EXECUTABLE_BYTES {
            return Err(PluginError::InvalidBundle(
                "native plugin executable is empty or too large".into(),
            ));
        }
        if self.assets.len() > MAX_ASSETS {
            return Err(PluginError::InvalidBundle("too many assets".into()));
        }
        let mut asset_bytes = 0_usize;
        for path in self.assets.keys() {
            validate_relative_path(path)?;
            if path.len() > 512 {
                return Err(PluginError::InvalidBundle("asset path is too long".into()));
            }
            asset_bytes = asset_bytes
                .checked_add(self.assets[path].len())
                .ok_or_else(|| PluginError::InvalidBundle("asset size overflow".into()))?;
        }
        if asset_bytes > MAX_ASSET_BYTES {
            return Err(PluginError::InvalidBundle("assets are too large".into()));
        }
        let manifest = self.manifest.canonical_bytes()?;
        let mut output = Vec::new();
        output.extend_from_slice(b"vbuff-native-plugin-bundle-v2\0");
        append_field(&mut output, &manifest)?;
        append_field(&mut output, &self.executable)?;
        append_u64(&mut output, self.assets.len())?;
        for (path, bytes) in &self.assets {
            append_field(&mut output, path.as_bytes())?;
            append_field(&mut output, bytes)?;
        }
        Ok(output)
    }

    pub fn hash(&self) -> Result<[u8; 32]> {
        Ok(*blake3::hash(&self.reproducible_bytes()?).as_bytes())
    }

    pub fn sign(&self, signing_key: &SigningKey) -> Result<SignedBundle> {
        let bundle_hash = self.hash()?;
        Ok(SignedBundle {
            bundle_hash,
            signer_public_key: signing_key.verifying_key().to_bytes(),
            signature: signing_key.sign(&bundle_hash).to_bytes().to_vec(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedBundle {
    pub bundle_hash: [u8; 32],
    pub signer_public_key: [u8; 32],
    pub signature: Vec<u8>,
}

impl SignedBundle {
    pub fn verify(&self, bundle: &PluginBundle) -> Result<()> {
        if self.bundle_hash != bundle.hash()? {
            return Err(PluginError::InvalidSignature);
        }
        let key = VerifyingKey::from_bytes(&self.signer_public_key)
            .map_err(|_| PluginError::InvalidSignature)?;
        let signature =
            Signature::from_slice(&self.signature).map_err(|_| PluginError::InvalidSignature)?;
        key.verify(&self.bundle_hash, &signature)
            .map_err(|_| PluginError::InvalidSignature)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedPlugin {
    pub version: String,
    pub bundle_hash: [u8; 32],
    pub signer_public_key: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginLock {
    pub schema_version: u16,
    pub plugins: BTreeMap<String, LockedPlugin>,
}

impl Default for PluginLock {
    fn default() -> Self {
        Self {
            schema_version: 1,
            plugins: BTreeMap::new(),
        }
    }
}

impl PluginLock {
    pub fn record(&mut self, bundle: &PluginBundle, signed: &SignedBundle) -> Result<()> {
        signed.verify(bundle)?;
        self.plugins.insert(
            bundle.manifest.id.clone(),
            LockedPlugin {
                version: bundle.manifest.version.clone(),
                bundle_hash: signed.bundle_hash,
                signer_public_key: signed.signer_public_key,
            },
        );
        Ok(())
    }

    pub fn verify(&self, bundle: &PluginBundle, signed: &SignedBundle) -> Result<()> {
        signed.verify(bundle)?;
        let locked = self
            .plugins
            .get(&bundle.manifest.id)
            .ok_or_else(|| PluginError::InvalidBundle("plugin is absent from lockfile".into()))?;
        if locked.version != bundle.manifest.version
            || locked.bundle_hash != signed.bundle_hash
            || locked.signer_public_key != signed.signer_public_key
        {
            return Err(PluginError::InvalidSignature);
        }
        Ok(())
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>> {
        serde_json::to_vec_pretty(self)
            .map_err(|error| PluginError::Serialization(error.to_string()))
    }
}

fn append_field(output: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    append_u64(output, bytes.len())?;
    output.extend_from_slice(bytes);
    Ok(())
}

fn append_u64(output: &mut Vec<u8>, value: usize) -> Result<()> {
    let value = u64::try_from(value)
        .map_err(|_| PluginError::InvalidBundle("bundle field is too large".into()))?;
    output.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::protocol::PROTOCOL_VERSION;

    fn bundle() -> PluginBundle {
        PluginBundle {
            manifest: PluginManifest {
                id: "dev.vbuff.sample".into(),
                name: "Sample".into(),
                version: "1.0.0".into(),
                protocol_version: PROTOCOL_VERSION,
                executable_path: "bin/plugin".into(),
                requested_capabilities: BTreeSet::new(),
                network_hosts: BTreeSet::new(),
                file_paths: BTreeSet::new(),
                process_commands: BTreeSet::new(),
            },
            executable: b"native-test-executable".to_vec(),
            assets: BTreeMap::from([
                ("icons/16.png".into(), vec![1, 2]),
                ("locale/en.json".into(), vec![3, 4]),
            ]),
        }
    }

    #[test]
    fn bundle_and_lockfile_are_reproducible_and_signed() {
        let bundle = bundle();
        assert_eq!(
            bundle.reproducible_bytes().unwrap(),
            bundle.reproducible_bytes().unwrap()
        );
        let signing_key = SigningKey::from_bytes(&[9; 32]);
        let signed = bundle.sign(&signing_key).unwrap();
        signed.verify(&bundle).unwrap();
        let mut lock = PluginLock::default();
        lock.record(&bundle, &signed).unwrap();
        lock.verify(&bundle, &signed).unwrap();
        assert_eq!(
            lock.canonical_json().unwrap(),
            lock.canonical_json().unwrap()
        );
    }

    #[test]
    fn signature_detects_executable_tampering() {
        let mut bundle = bundle();
        let signed = bundle.sign(&SigningKey::from_bytes(&[4; 32])).unwrap();
        bundle.executable.push(0);
        assert_eq!(signed.verify(&bundle), Err(PluginError::InvalidSignature));
    }
}
