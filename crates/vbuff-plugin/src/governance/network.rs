use std::net::IpAddr;

use url::{Host, Url};

use crate::{ActionPermissionRequest, PluginCapability, PluginError, PluginManifest, Result};

const MAX_FETCH_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkFetchPolicy {
    pub max_response_bytes: u64,
}

impl NetworkFetchPolicy {
    pub fn new(max_response_bytes: u64) -> Result<Self> {
        if max_response_bytes == 0 || max_response_bytes > MAX_FETCH_BYTES {
            return Err(PluginError::CapabilityDenied(
                "network response limit is invalid".into(),
            ));
        }
        Ok(Self { max_response_bytes })
    }

    pub fn authorize(
        &self,
        manifest: &PluginManifest,
        request: &ActionPermissionRequest,
        url: &Url,
    ) -> Result<AuthorizedFetch> {
        request.validate(manifest)?;
        if !request.required.contains(&PluginCapability::Network) {
            return Err(PluginError::CapabilityDenied(
                "action did not request network access".into(),
            ));
        }
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || url.port().is_some_and(|port| port != 443)
        {
            return Err(PluginError::CapabilityDenied(
                "network fetch requires credential-free HTTPS URL".into(),
            ));
        }
        let host = match url.host() {
            Some(Host::Domain(host)) if is_nonlocal_domain(host) => host,
            _ => {
                return Err(PluginError::CapabilityDenied(
                    "network fetch requires a non-local domain host".into(),
                ));
            }
        };
        if !request.network_hosts.contains(host) {
            return Err(PluginError::CapabilityDenied(
                "network host is outside the exact action scope".into(),
            ));
        }
        Ok(AuthorizedFetch {
            host: host.to_owned(),
            port: 443,
            max_response_bytes: self.max_response_bytes,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizedFetch {
    pub host: String,
    pub port: u16,
    pub max_response_bytes: u64,
}

impl AuthorizedFetch {
    /// Must be checked for every DNS answer and redirect target immediately
    /// before the future host opens a socket.
    pub fn allows_resolved_address(&self, address: IpAddr) -> bool {
        is_public_address(address)
    }
}

pub(super) fn is_nonlocal_domain(host: &str) -> bool {
    host.contains('.')
        && host != "localhost"
        && !host.ends_with(".localhost")
        && !host.ends_with(".local")
}

fn is_public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let octets = address.octets();
            !address.is_private()
                && !address.is_loopback()
                && !address.is_link_local()
                && !address.is_broadcast()
                && !address.is_documentation()
                && !address.is_unspecified()
                && !address.is_multicast()
                && octets[0] != 0
                && !(octets[0] == 100 && (64..=127).contains(&octets[1]))
                && !(octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
                && !(octets[0] == 192 && octets[1] == 88 && octets[2] == 99)
                && !(octets[0] == 198 && matches!(octets[1], 18 | 19))
                && octets[0] < 224
        }
        IpAddr::V6(address) => {
            if let Some(mapped) = address.to_ipv4_mapped() {
                return is_public_address(IpAddr::V4(mapped));
            }
            let segments = address.segments();
            !address.is_loopback()
                && !address.is_unspecified()
                && !address.is_multicast()
                && (segments[0] & 0xfe00) != 0xfc00
                && (segments[0] & 0xffc0) != 0xfe80
                && (segments[0] & 0xffc0) != 0xfec0
                && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
        }
    }
}
