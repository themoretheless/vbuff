use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolRange {
    pub minimum: u16,
    pub maximum: u16,
}

impl ProtocolRange {
    pub const fn contains(self, version: u16) -> bool {
        self.minimum <= version && version <= self.maximum
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    ReadHistory,
    ReadSensitiveHistory,
    MutateHistory,
    SubscribeEvents,
    DryRunTransforms,
    BatchMutations,
    PluginManagement,
    Diagnostics,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientHello {
    pub client_name: String,
    pub protocol: ProtocolRange,
    pub requested: BTreeSet<Capability>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerPolicy {
    pub protocol: ProtocolRange,
    pub available: BTreeSet<Capability>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerWelcome {
    pub protocol_version: u16,
    pub granted: BTreeSet<Capability>,
    pub denied: BTreeSet<Capability>,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum HandshakeError {
    #[error("invalid protocol range")]
    InvalidRange,
    #[error("client and server protocol ranges do not overlap")]
    IncompatibleProtocol,
    #[error("client name is empty or too long")]
    InvalidClientName,
}

pub fn negotiate(
    hello: &ClientHello,
    server: &ServerPolicy,
) -> Result<ServerWelcome, HandshakeError> {
    if hello.client_name.trim().is_empty()
        || hello.client_name.len() > 128
        || hello.client_name.chars().any(char::is_control)
    {
        return Err(HandshakeError::InvalidClientName);
    }
    if hello.protocol.minimum > hello.protocol.maximum
        || server.protocol.minimum > server.protocol.maximum
    {
        return Err(HandshakeError::InvalidRange);
    }
    let minimum = hello.protocol.minimum.max(server.protocol.minimum);
    let maximum = hello.protocol.maximum.min(server.protocol.maximum);
    if minimum > maximum {
        return Err(HandshakeError::IncompatibleProtocol);
    }
    Ok(ServerWelcome {
        protocol_version: maximum,
        granted: hello
            .requested
            .intersection(&server.available)
            .copied()
            .collect(),
        denied: hello
            .requested
            .difference(&server.available)
            .copied()
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiation_selects_highest_overlap_and_denies_unavailable_caps() {
        let hello = ClientHello {
            client_name: "cli".into(),
            protocol: ProtocolRange {
                minimum: 1,
                maximum: 3,
            },
            requested: BTreeSet::from([Capability::ReadHistory, Capability::PluginManagement]),
        };
        let welcome = negotiate(
            &hello,
            &ServerPolicy {
                protocol: ProtocolRange {
                    minimum: 2,
                    maximum: 4,
                },
                available: BTreeSet::from([Capability::ReadHistory]),
            },
        )
        .unwrap();
        assert_eq!(welcome.protocol_version, 3);
        assert_eq!(welcome.granted, BTreeSet::from([Capability::ReadHistory]));
        assert_eq!(
            welcome.denied,
            BTreeSet::from([Capability::PluginManagement])
        );
    }
}
