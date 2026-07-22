use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use chrono::{DateTime, Utc};

const MAX_SOURCE_BYTES: usize = 512;
const MAX_SOURCE_CHOICES: usize = 1_024;
const MAX_DEVICE_BYTES: usize = 512;
const MAX_DEVICE_TIMEOUTS: usize = 256;
const MIN_UNLOCK_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_UNLOCK_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);
const SOURCE_HASH_DOMAIN: &[u8] = b"vbuff-sensitive-source-v1\0";
const DEVICE_HASH_DOMAIN: &[u8] = b"vbuff-unlock-device-v1\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SensitiveSourceChoice {
    Skip,
    Ephemeral,
    Normal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SensitiveSourceDecision {
    CaptureNormally,
    Prompt,
    Skip,
    CaptureEphemeral { ttl: Duration },
}

#[derive(Clone)]
pub struct SensitiveSourceConsent {
    choices: BTreeMap<[u8; 32], SensitiveSourceChoice>,
    ephemeral_ttl: Duration,
}

impl fmt::Debug for SensitiveSourceConsent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SensitiveSourceConsent")
            .field("choice_count", &self.choices.len())
            .field("ephemeral_ttl", &self.ephemeral_ttl)
            .finish()
    }
}

impl SensitiveSourceConsent {
    pub fn new(ephemeral_ttl: Duration) -> Option<Self> {
        (!ephemeral_ttl.is_zero() && ephemeral_ttl <= Duration::from_secs(24 * 60 * 60)).then(
            || Self {
                choices: BTreeMap::new(),
                ephemeral_ttl,
            },
        )
    }

    pub fn decide(&self, source_app: &str, sensitive_source: bool) -> SensitiveSourceDecision {
        if !sensitive_source {
            return SensitiveSourceDecision::CaptureNormally;
        }
        let Some(key) = bounded_identity_hash(SOURCE_HASH_DOMAIN, source_app, MAX_SOURCE_BYTES)
        else {
            return SensitiveSourceDecision::Skip;
        };
        match self.choices.get(&key) {
            None => SensitiveSourceDecision::Prompt,
            Some(SensitiveSourceChoice::Skip) => SensitiveSourceDecision::Skip,
            Some(SensitiveSourceChoice::Ephemeral) => SensitiveSourceDecision::CaptureEphemeral {
                ttl: self.ephemeral_ttl,
            },
            Some(SensitiveSourceChoice::Normal) => SensitiveSourceDecision::CaptureNormally,
        }
    }

    pub fn remember(&mut self, source_app: &str, choice: SensitiveSourceChoice) -> bool {
        let Some(key) = bounded_identity_hash(SOURCE_HASH_DOMAIN, source_app, MAX_SOURCE_BYTES)
        else {
            return false;
        };
        if !self.choices.contains_key(&key) && self.choices.len() >= MAX_SOURCE_CHOICES {
            return false;
        }
        self.choices.insert(key, choice);
        true
    }

    pub fn forget(&mut self, source_app: &str) -> bool {
        bounded_identity_hash(SOURCE_HASH_DOMAIN, source_app, MAX_SOURCE_BYTES)
            .is_some_and(|key| self.choices.remove(&key).is_some())
    }
}

impl Default for SensitiveSourceConsent {
    fn default() -> Self {
        Self::new(Duration::from_secs(60)).expect("default consent TTL is valid")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalAction {
    Sync,
    Export,
    Webhook,
    Plugin,
    NetworkRequest,
    LocalCapture,
    LocalPaste,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalOnlyWorkspacePolicy {
    strict: bool,
}

impl LocalOnlyWorkspacePolicy {
    pub const fn new(strict: bool) -> Self {
        Self { strict }
    }

    pub const fn allows(self, action: ExternalAction) -> bool {
        !self.strict
            || matches!(
                action,
                ExternalAction::LocalCapture | ExternalAction::LocalPaste
            )
    }

    pub const fn strict(self) -> bool {
        self.strict
    }
}

#[derive(Clone, Default)]
pub struct UnlockTimeouts {
    timeouts: BTreeMap<[u8; 32], Duration>,
}

impl fmt::Debug for UnlockTimeouts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UnlockTimeouts")
            .field("device_count", &self.timeouts.len())
            .finish()
    }
}

impl UnlockTimeouts {
    pub fn set(&mut self, device_id: &str, timeout: Duration) -> bool {
        if !(MIN_UNLOCK_TIMEOUT..=MAX_UNLOCK_TIMEOUT).contains(&timeout) {
            return false;
        }
        let Some(key) = bounded_identity_hash(DEVICE_HASH_DOMAIN, device_id, MAX_DEVICE_BYTES)
        else {
            return false;
        };
        if !self.timeouts.contains_key(&key) && self.timeouts.len() >= MAX_DEVICE_TIMEOUTS {
            return false;
        }
        self.timeouts.insert(key, timeout);
        true
    }

    pub fn timeout(&self, device_id: &str) -> Option<Duration> {
        let key = bounded_identity_hash(DEVICE_HASH_DOMAIN, device_id, MAX_DEVICE_BYTES)?;
        self.timeouts.get(&key).copied()
    }

    pub fn should_lock(&self, device_id: &str, idle_for: Duration) -> bool {
        self.timeout(device_id)
            .is_none_or(|timeout| idle_for >= timeout)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EphemeralCountdown {
    Expired,
    Remaining { seconds: u64 },
}

impl EphemeralCountdown {
    pub fn between(now: DateTime<Utc>, expires_at: DateTime<Utc>) -> Self {
        let remaining_ms = expires_at.signed_duration_since(now).num_milliseconds();
        if remaining_ms <= 0 {
            Self::Expired
        } else {
            Self::Remaining {
                seconds: (remaining_ms as u64).saturating_add(999) / 1_000,
            }
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Expired => "expired".into(),
            Self::Remaining { seconds } if seconds < 60 => format!("{seconds}s"),
            Self::Remaining { seconds } => {
                let minutes = seconds.saturating_add(59) / 60;
                format!("{minutes}m")
            }
        }
    }
}

fn bounded_identity_hash(domain: &[u8], value: &str, maximum_bytes: usize) -> Option<[u8; 32]> {
    let value = value.trim();
    if value.is_empty() || value.len() > maximum_bytes || value.chars().any(char::is_control) {
        return None;
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(value.as_bytes());
    Some(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    #[test]
    fn unknown_sensitive_sources_prompt_and_remember_only_hashed_identity() {
        let mut consent = SensitiveSourceConsent::default();
        assert_eq!(
            consent.decide("com.example.passwords", true),
            SensitiveSourceDecision::Prompt
        );
        assert!(consent.remember("com.example.passwords", SensitiveSourceChoice::Ephemeral));
        assert_eq!(
            consent.decide("com.example.passwords", true),
            SensitiveSourceDecision::CaptureEphemeral {
                ttl: Duration::from_secs(60)
            }
        );
        assert!(!format!("{consent:?}").contains("passwords"));
        assert_eq!(
            consent.decide("anything", false),
            SensitiveSourceDecision::CaptureNormally
        );
    }

    #[test]
    fn strict_local_only_policy_has_no_external_escape_hatch() {
        let policy = LocalOnlyWorkspacePolicy::new(true);
        for action in [
            ExternalAction::Sync,
            ExternalAction::Export,
            ExternalAction::Webhook,
            ExternalAction::Plugin,
            ExternalAction::NetworkRequest,
        ] {
            assert!(!policy.allows(action));
        }
        assert!(policy.allows(ExternalAction::LocalCapture));
        assert!(policy.allows(ExternalAction::LocalPaste));
    }

    #[test]
    fn unlock_timeouts_are_per_device_bounded_and_fail_closed() {
        let mut timeouts = UnlockTimeouts::default();
        assert!(timeouts.set("desktop", Duration::from_secs(10 * 60)));
        assert!(timeouts.set("travel", Duration::from_secs(60)));
        assert!(!timeouts.set("bad", Duration::from_secs(1)));
        assert!(!timeouts.should_lock("desktop", Duration::from_secs(9 * 60)));
        assert!(timeouts.should_lock("travel", Duration::from_secs(61)));
        assert!(timeouts.should_lock("unknown", Duration::ZERO));
        assert!(!format!("{timeouts:?}").contains("desktop"));
    }

    #[test]
    fn countdown_rounds_up_and_expires_at_the_boundary() {
        let now = Utc.timestamp_opt(1_000, 0).unwrap();
        assert_eq!(
            EphemeralCountdown::between(now, now + chrono::Duration::milliseconds(1)).label(),
            "1s"
        );
        assert_eq!(EphemeralCountdown::between(now, now).label(), "expired");
        assert_eq!(
            EphemeralCountdown::between(now, now + chrono::Duration::seconds(61)).label(),
            "2m"
        );
    }
}
