use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{PluginError, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
    ReadClipContent,
    WriteDerivedClip,
    SubscribeEvents,
    Network,
    FileRead,
    FileWrite,
    ProcessSpawn,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub abi_version: u16,
    pub component_path: String,
    pub requested_capabilities: BTreeSet<PluginCapability>,
    #[serde(default)]
    pub network_hosts: BTreeSet<String>,
    #[serde(default)]
    pub file_paths: BTreeSet<String>,
    #[serde(default)]
    pub process_commands: BTreeSet<String>,
}

impl PluginManifest {
    pub fn validate(&self) -> Result<()> {
        let valid_id = !self.id.is_empty()
            && self.id.len() <= 128
            && self.id.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-' | b'_')
            });
        if !valid_id {
            return Err(PluginError::InvalidManifest("invalid plugin id".into()));
        }
        if self.name.trim().is_empty()
            || self.name.len() > 128
            || self.name.chars().any(char::is_control)
        {
            return Err(PluginError::InvalidManifest("invalid display name".into()));
        }
        if !valid_version(&self.version) {
            return Err(PluginError::InvalidManifest("version must be x.y.z".into()));
        }
        if self.abi_version != crate::component::ABI_VERSION {
            return Err(PluginError::InvalidManifest(
                "unsupported component ABI".into(),
            ));
        }
        if self.version.len() > 64 {
            return Err(PluginError::InvalidManifest("version is too long".into()));
        }
        if self.component_path.len() > 512 {
            return Err(PluginError::InvalidManifest(
                "component path is too long".into(),
            ));
        }
        validate_relative_path(&self.component_path)?;
        if !self.component_path.ends_with(".wasm") {
            return Err(PluginError::InvalidManifest(
                "component path must end in .wasm".into(),
            ));
        }
        let requests_network = self
            .requested_capabilities
            .contains(&PluginCapability::Network);
        if requests_network != !self.network_hosts.is_empty()
            || self.network_hosts.len() > 64
            || self.network_hosts.iter().any(|host| !valid_host(host))
        {
            return Err(PluginError::InvalidManifest(
                "network capability requires 1-64 valid explicit hosts".into(),
            ));
        }
        let requests_files = self
            .requested_capabilities
            .contains(&PluginCapability::FileRead)
            || self
                .requested_capabilities
                .contains(&PluginCapability::FileWrite);
        if requests_files != !self.file_paths.is_empty()
            || self.file_paths.len() > 64
            || self.file_paths.iter().any(|path| !valid_scope_path(path))
        {
            return Err(PluginError::InvalidManifest(
                "file capability requires 1-64 valid explicit paths".into(),
            ));
        }
        let requests_process = self
            .requested_capabilities
            .contains(&PluginCapability::ProcessSpawn);
        if requests_process != !self.process_commands.is_empty()
            || self.process_commands.len() > 32
            || self
                .process_commands
                .iter()
                .any(|command| !valid_command_id(command))
        {
            return Err(PluginError::InvalidManifest(
                "process capability requires 1-32 explicit host command ids".into(),
            ));
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|error| PluginError::Serialization(error.to_string()))
    }

    pub fn hash(&self) -> Result<[u8; 32]> {
        Ok(*blake3::hash(&self.canonical_bytes()?).as_bytes())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityGrant {
    pub plugin_id: String,
    pub manifest_hash: [u8; 32],
    pub granted: BTreeSet<PluginCapability>,
}

impl CapabilityGrant {
    pub fn authorize(&self, manifest: &PluginManifest) -> Result<()> {
        if self.plugin_id != manifest.id || self.manifest_hash != manifest.hash()? {
            return Err(PluginError::CapabilityDenied(
                "grant does not match this manifest revision".into(),
            ));
        }
        if let Some(missing) = manifest
            .requested_capabilities
            .difference(&self.granted)
            .next()
        {
            return Err(PluginError::CapabilityDenied(format!("{missing:?}")));
        }
        if let Some(unrequested) = self
            .granted
            .difference(&manifest.requested_capabilities)
            .next()
        {
            return Err(PluginError::CapabilityDenied(format!(
                "unrequested grant: {unrequested:?}"
            )));
        }
        Ok(())
    }
}

pub(crate) fn validate_relative_path(path: &str) -> Result<()> {
    let valid = !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && !path.contains(':')
        && !path.contains('\0')
        && !path
            .split(['/', '\\'])
            .any(|component| component.is_empty() || matches!(component, "." | ".."));
    if valid {
        Ok(())
    } else {
        Err(PluginError::InvalidBundle(format!(
            "unsafe relative path: {path}"
        )))
    }
}

fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host == host.trim()
        && !host.chars().any(char::is_control)
        && url::Host::parse(host).is_ok()
}

fn valid_scope_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 1_024
        && !path.contains('\0')
        && !path.chars().any(char::is_control)
        && !path.split(['/', '\\']).any(|component| component == "..")
}

fn valid_command_id(command: &str) -> bool {
    !command.is_empty()
        && command.len() <= 128
        && command
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_version(version: &str) -> bool {
    let mut parts = version.split('.');
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

    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "dev.vbuff.sample".into(),
            name: "Sample".into(),
            version: "1.2.3".into(),
            abi_version: crate::component::ABI_VERSION,
            component_path: "plugin.wasm".into(),
            requested_capabilities: BTreeSet::from([PluginCapability::ReadClipContent]),
            network_hosts: BTreeSet::new(),
            file_paths: BTreeSet::new(),
            process_commands: BTreeSet::new(),
        }
    }

    #[test]
    fn grants_are_bound_to_manifest_revision_and_cover_every_request() {
        let manifest = manifest();
        let grant = CapabilityGrant {
            plugin_id: manifest.id.clone(),
            manifest_hash: manifest.hash().unwrap(),
            granted: manifest.requested_capabilities.clone(),
        };
        assert!(grant.authorize(&manifest).is_ok());
        let mut changed = manifest;
        changed.version = "1.2.4".into();
        assert!(grant.authorize(&changed).is_err());
    }

    #[test]
    fn traversal_component_paths_are_rejected() {
        let mut manifest = manifest();
        manifest.component_path = "../plugin.wasm".into();
        assert!(manifest.validate().is_err());
        manifest.component_path = "C:plugin.wasm".into();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn network_and_file_capabilities_require_explicit_scopes() {
        let mut manifest = manifest();
        manifest
            .requested_capabilities
            .insert(PluginCapability::Network);
        assert!(manifest.validate().is_err());
        manifest.network_hosts.insert("api.example.com".into());
        assert!(manifest.validate().is_ok());

        manifest
            .requested_capabilities
            .insert(PluginCapability::ProcessSpawn);
        assert!(manifest.validate().is_err());
        manifest.process_commands.insert("format-json".into());
        assert!(manifest.validate().is_ok());
    }
}
