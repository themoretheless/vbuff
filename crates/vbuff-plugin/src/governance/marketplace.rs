use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use url::{Host, Url};

use super::{invalid, network::is_nonlocal_domain, valid_id};
use crate::{PluginCapability, PluginError, PluginManifest, Result};

const MARKETPLACE_SCHEMA: u16 = 1;
const MAX_EXAMPLES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketplaceCategory {
    DeveloperTools,
    Formatting,
    Productivity,
    Security,
    Translation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketplaceExample {
    pub title: String,
    pub action_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketplaceMetadata {
    pub schema: u16,
    pub plugin_id: String,
    pub display_name: String,
    pub summary: String,
    pub license: String,
    pub categories: BTreeSet<MarketplaceCategory>,
    pub declared_capabilities: BTreeSet<PluginCapability>,
    pub minimum_protocol: u16,
    pub maximum_protocol: u16,
    pub documentation_url: Option<String>,
    pub examples: Vec<MarketplaceExample>,
}

impl MarketplaceMetadata {
    pub fn validate(&self, manifest: &PluginManifest) -> Result<()> {
        manifest.validate()?;
        if self.schema != MARKETPLACE_SCHEMA
            || self.plugin_id != manifest.id
            || self.display_name != manifest.name
            || self.summary.trim().is_empty()
            || self.summary.len() > 280
            || self.summary.chars().any(char::is_control)
            || !valid_license(&self.license)
            || self.categories.is_empty()
            || self.categories.len() > 3
            || self.declared_capabilities != manifest.requested_capabilities
            || self.minimum_protocol == 0
            || self.minimum_protocol > manifest.protocol_version
            || self.maximum_protocol < manifest.protocol_version
            || self.maximum_protocol < self.minimum_protocol
            || self.examples.len() > MAX_EXAMPLES
        {
            return invalid("marketplace metadata is invalid or misleading");
        }
        for example in &self.examples {
            if example.title.trim().is_empty()
                || example.title.len() > 80
                || example.title.chars().any(char::is_control)
                || !valid_id(&example.action_id)
            {
                return invalid("marketplace example is invalid");
            }
        }
        if let Some(documentation_url) = &self.documentation_url {
            let parsed = Url::parse(documentation_url)
                .map_err(|_| PluginError::InvalidBundle("documentation URL is invalid".into()))?;
            if parsed.scheme() != "https"
                || !parsed.username().is_empty()
                || parsed.password().is_some()
                || parsed.fragment().is_some()
                || parsed.port().is_some_and(|port| port != 443)
                || !matches!(parsed.host(), Some(Host::Domain(host)) if is_nonlocal_domain(host))
            {
                return invalid("documentation URL must use credential-free HTTPS");
            }
        }
        Ok(())
    }
}

fn valid_license(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
}
