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
    pub protocol_version: u16,
    pub executable_path: String,
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
        if self.protocol_version != crate::protocol::PROTOCOL_VERSION {
            return Err(PluginError::InvalidManifest(
                "unsupported native plugin protocol".into(),
            ));
        }
        if self.version.len() > 64 {
            return Err(PluginError::InvalidManifest("version is too long".into()));
        }
        if self.executable_path.len() > 512 {
            return Err(PluginError::InvalidManifest(
                "executable path is too long".into(),
            ));
        }
        validate_relative_path(&self.executable_path)?;
        if !self.executable_path.starts_with("bin/") {
            return Err(PluginError::InvalidManifest(
                "native plugin executable must be stored under bin/".into(),
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionPermissionRequest {
    pub action_id: String,
    pub required: BTreeSet<PluginCapability>,
    #[serde(default)]
    pub network_hosts: BTreeSet<String>,
    #[serde(default)]
    pub file_paths: BTreeSet<String>,
    #[serde(default)]
    pub process_commands: BTreeSet<String>,
}

impl ActionPermissionRequest {
    pub fn validate(&self, manifest: &PluginManifest) -> Result<()> {
        manifest.validate()?;
        if !valid_command_id(&self.action_id) {
            return Err(PluginError::InvalidManifest("invalid action id".into()));
        }
        if !self.required.is_subset(&manifest.requested_capabilities) {
            return Err(PluginError::CapabilityDenied(
                "action requests a capability outside its manifest".into(),
            ));
        }
        validate_action_scope(
            self.required.contains(&PluginCapability::Network),
            &self.network_hosts,
            &manifest.network_hosts,
            64,
            valid_host,
            "network",
        )?;
        validate_action_scope(
            self.required.contains(&PluginCapability::FileRead)
                || self.required.contains(&PluginCapability::FileWrite),
            &self.file_paths,
            &manifest.file_paths,
            64,
            valid_scope_path,
            "file",
        )?;
        validate_action_scope(
            self.required.contains(&PluginCapability::ProcessSpawn),
            &self.process_commands,
            &manifest.process_commands,
            32,
            valid_command_id,
            "process",
        )?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionCapabilityGrant {
    pub plugin_id: String,
    pub manifest_hash: [u8; 32],
    pub action_id: String,
    pub granted: BTreeSet<PluginCapability>,
    #[serde(default)]
    pub network_hosts: BTreeSet<String>,
    #[serde(default)]
    pub file_paths: BTreeSet<String>,
    #[serde(default)]
    pub process_commands: BTreeSet<String>,
}

impl ActionCapabilityGrant {
    pub fn authorize(
        &self,
        manifest: &PluginManifest,
        request: &ActionPermissionRequest,
    ) -> Result<()> {
        request.validate(manifest)?;
        if self.plugin_id != manifest.id
            || self.manifest_hash != manifest.hash()?
            || self.action_id != request.action_id
        {
            return Err(PluginError::CapabilityDenied(
                "action grant does not match this action revision".into(),
            ));
        }
        if self.granted != request.required
            || self.network_hosts != request.network_hosts
            || self.file_paths != request.file_paths
            || self.process_commands != request.process_commands
        {
            return Err(PluginError::CapabilityDenied(
                "action grant must exactly match its requested sandbox".into(),
            ));
        }
        Ok(())
    }
}

fn validate_action_scope(
    required: bool,
    requested: &BTreeSet<String>,
    manifest_scope: &BTreeSet<String>,
    max_items: usize,
    validator: fn(&str) -> bool,
    label: &str,
) -> Result<()> {
    if required != !requested.is_empty()
        || requested.len() > max_items
        || !requested.is_subset(manifest_scope)
        || requested.iter().any(|value| !validator(value))
    {
        return Err(PluginError::CapabilityDenied(format!(
            "invalid per-action {label} scope"
        )));
    }
    Ok(())
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
            protocol_version: crate::protocol::PROTOCOL_VERSION,
            executable_path: "bin/plugin".into(),
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
    fn traversal_and_non_bin_executable_paths_are_rejected() {
        let mut manifest = manifest();
        manifest.executable_path = "../plugin".into();
        assert!(manifest.validate().is_err());
        manifest.executable_path = "C:plugin.exe".into();
        assert!(manifest.validate().is_err());
        manifest.executable_path = "plugin".into();
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

    #[test]
    fn action_grants_cannot_inherit_unrequested_manifest_scope() {
        let mut manifest = manifest();
        manifest
            .requested_capabilities
            .insert(PluginCapability::Network);
        manifest.network_hosts =
            BTreeSet::from(["api.example.com".into(), "unused.example.com".into()]);
        let request = ActionPermissionRequest {
            action_id: "summarize".into(),
            required: BTreeSet::from([
                PluginCapability::ReadClipContent,
                PluginCapability::Network,
            ]),
            network_hosts: BTreeSet::from(["api.example.com".into()]),
            file_paths: BTreeSet::new(),
            process_commands: BTreeSet::new(),
        };
        let grant = ActionCapabilityGrant {
            plugin_id: manifest.id.clone(),
            manifest_hash: manifest.hash().unwrap(),
            action_id: request.action_id.clone(),
            granted: request.required.clone(),
            network_hosts: request.network_hosts.clone(),
            file_paths: BTreeSet::new(),
            process_commands: BTreeSet::new(),
        };
        assert!(grant.authorize(&manifest, &request).is_ok());

        let mut overbroad = grant;
        overbroad.network_hosts.insert("unused.example.com".into());
        assert!(overbroad.authorize(&manifest, &request).is_err());
    }
}
