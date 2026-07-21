use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::IntegrationContractError;

const MAX_OSC52_PAYLOAD_BYTES: u64 = 16 * 1_024 * 1_024;
const MAX_ALLOWED_REMOTE_HOSTS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Osc52Target {
    Clipboard,
    PrimarySelection,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Osc52Observation {
    pub payload_hash: [u8; 32],
    pub payload_bytes: u64,
    pub terminal_app_hash: [u8; 32],
    pub remote_host_hash: Option<[u8; 32]>,
    pub session_hash: Option<[u8; 32]>,
    pub target: Osc52Target,
}

impl fmt::Debug for Osc52Observation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Osc52Observation")
            .field("payload_hash", &"[redacted]")
            .field("payload_bytes", &self.payload_bytes)
            .field("has_remote_host", &self.remote_host_hash.is_some())
            .field("has_session", &self.session_hash.is_some())
            .field("target", &self.target)
            .finish()
    }
}

impl Osc52Observation {
    pub fn from_metadata(
        payload: &[u8],
        terminal_app: &str,
        remote_host: Option<&str>,
        session: Option<&str>,
        target: Osc52Target,
    ) -> Result<Self, IntegrationContractError> {
        let payload_bytes =
            u64::try_from(payload.len()).map_err(|_| IntegrationContractError::InvalidField)?;
        if payload.is_empty()
            || payload_bytes > MAX_OSC52_PAYLOAD_BYTES
            || terminal_app.is_empty()
            || terminal_app.len() > 256
            || terminal_app.chars().any(char::is_control)
            || remote_host.is_some_and(invalid_identifier)
            || session.is_some_and(invalid_identifier)
        {
            return Err(IntegrationContractError::InvalidField);
        }
        Ok(Self {
            payload_hash: *blake3::hash(payload).as_bytes(),
            payload_bytes,
            terminal_app_hash: *blake3::hash(terminal_app.as_bytes()).as_bytes(),
            remote_host_hash: remote_host.map(|value| *blake3::hash(value.as_bytes()).as_bytes()),
            session_hash: session.map(|value| *blake3::hash(value.as_bytes()).as_bytes()),
            target,
        })
    }
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Osc52Policy {
    pub maximum_bytes: u64,
    pub allow_remote: bool,
    pub allowed_remote_hosts: BTreeSet<[u8; 32]>,
}

impl fmt::Debug for Osc52Policy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Osc52Policy")
            .field("maximum_bytes", &self.maximum_bytes)
            .field("allow_remote", &self.allow_remote)
            .field(
                "allowed_remote_host_count",
                &self.allowed_remote_hosts.len(),
            )
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Osc52Decision {
    Allow,
    BlockOversize,
    BlockRemote,
    BlockUnknownRemote,
    BlockInvalidPolicy,
}

impl Osc52Policy {
    pub fn validate(&self) -> Result<(), IntegrationContractError> {
        if self.maximum_bytes == 0
            || self.maximum_bytes > MAX_OSC52_PAYLOAD_BYTES
            || self.allowed_remote_hosts.len() > MAX_ALLOWED_REMOTE_HOSTS
            || self
                .allowed_remote_hosts
                .iter()
                .any(|host| host.iter().all(|byte| *byte == 0))
            || (!self.allow_remote && !self.allowed_remote_hosts.is_empty())
        {
            return Err(IntegrationContractError::InvalidField);
        }
        Ok(())
    }

    pub fn evaluate(&self, observation: &Osc52Observation) -> Osc52Decision {
        if self.validate().is_err() || !valid_observation(observation) {
            return Osc52Decision::BlockInvalidPolicy;
        }
        if observation.payload_bytes > self.maximum_bytes {
            return Osc52Decision::BlockOversize;
        }
        let Some(remote_host) = observation.remote_host_hash else {
            return Osc52Decision::Allow;
        };
        if !self.allow_remote {
            return Osc52Decision::BlockRemote;
        }
        if !self.allowed_remote_hosts.contains(&remote_host) {
            return Osc52Decision::BlockUnknownRemote;
        }
        Osc52Decision::Allow
    }
}

fn valid_observation(observation: &Osc52Observation) -> bool {
    observation.payload_bytes > 0
        && observation.payload_bytes <= MAX_OSC52_PAYLOAD_BYTES
        && observation.payload_hash.iter().any(|byte| *byte != 0)
        && observation.terminal_app_hash.iter().any(|byte| *byte != 0)
        && observation
            .remote_host_hash
            .is_none_or(|hash| hash.iter().any(|byte| *byte != 0))
        && observation
            .session_hash
            .is_none_or(|hash| hash.iter().any(|byte| *byte != 0))
}

fn invalid_identifier(value: &str) -> bool {
    value.is_empty()
        || value.len() > 256
        || value.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc52_attaches_hashed_origin_and_denies_remote_by_default() {
        let observation = Osc52Observation::from_metadata(
            b"private remote value",
            "terminal",
            Some("server.example"),
            Some("tmux-4"),
            Osc52Target::Clipboard,
        )
        .unwrap();
        let policy = Osc52Policy {
            maximum_bytes: 1_024,
            ..Osc52Policy::default()
        };
        assert_eq!(policy.evaluate(&observation), Osc52Decision::BlockRemote);
        assert!(!format!("{observation:?}").contains("private remote value"));
        assert!(!format!("{observation:?}").contains("server.example"));

        let mut allowed = policy;
        allowed.allow_remote = true;
        allowed
            .allowed_remote_hosts
            .insert(observation.remote_host_hash.unwrap());
        assert_eq!(allowed.evaluate(&observation), Osc52Decision::Allow);
        assert!(!format!("{allowed:?}").contains("server.example"));

        let invalid = Osc52Policy {
            maximum_bytes: 1_024,
            allow_remote: false,
            allowed_remote_hosts: BTreeSet::from([[1; 32]]),
        };
        assert_eq!(
            invalid.evaluate(&observation),
            Osc52Decision::BlockInvalidPolicy
        );
        assert!(
            Osc52Observation::from_metadata(
                &vec![0; usize::try_from(MAX_OSC52_PAYLOAD_BYTES).unwrap() + 1],
                "terminal",
                None,
                None,
                Osc52Target::Clipboard,
            )
            .is_err()
        );
    }
}
