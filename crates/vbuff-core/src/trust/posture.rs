use std::collections::BTreeMap;
use std::fmt;

use chacha20poly1305::aead::{Aead as _, KeyInit as _, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};

const CRASH_AAD: &[u8] = b"vbuff-crash-diagnostics-v1";
const MAX_RULE_COUNTERS: usize = 1_024;
const MAX_RULE_ID_BYTES: usize = 96;
const MAX_CRASH_CIPHERTEXT_BYTES: usize = 64 * 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct PrivacyPostureInput {
    pub encryption_at_rest: bool,
    pub strict_local_only: bool,
    pub sensitive_memory_only: bool,
    pub telemetry_enabled: bool,
    pub sync_enabled: bool,
    pub denied_source_count: u32,
    pub retention_days: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyScoreLevel {
    Strong,
    Balanced,
    NeedsAttention,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PrivacyScoreFactor {
    pub key: &'static str,
    pub points: i8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PrivacyScore {
    pub value: u8,
    pub level: PrivacyScoreLevel,
    pub factors: Vec<PrivacyScoreFactor>,
}

impl PrivacyScore {
    pub fn calculate(input: PrivacyPostureInput) -> Self {
        let mut factors = Vec::with_capacity(7);
        factor(
            &mut factors,
            "encryption_at_rest",
            if input.encryption_at_rest { 24 } else { -24 },
        );
        factor(
            &mut factors,
            "strict_local_only",
            if input.strict_local_only { 18 } else { 0 },
        );
        factor(
            &mut factors,
            "sensitive_memory_only",
            if input.sensitive_memory_only { 16 } else { -8 },
        );
        factor(
            &mut factors,
            "telemetry",
            if input.telemetry_enabled { -10 } else { 8 },
        );
        factor(
            &mut factors,
            "sync",
            if input.sync_enabled { -8 } else { 8 },
        );
        factor(
            &mut factors,
            "denied_sources",
            if input.denied_source_count > 0 { 8 } else { 0 },
        );
        let retention_points = match input.retention_days {
            Some(0..=7) => 12,
            Some(8..=30) => 6,
            Some(31..=90) => 0,
            Some(_) => -8,
            None => -12,
        };
        factor(&mut factors, "retention", retention_points);

        let value = (50_i16
            + factors
                .iter()
                .map(|factor| i16::from(factor.points))
                .sum::<i16>())
        .clamp(0, 100) as u8;
        let level = match value {
            80..=100 => PrivacyScoreLevel::Strong,
            55..=79 => PrivacyScoreLevel::Balanced,
            _ => PrivacyScoreLevel::NeedsAttention,
        };
        Self {
            value,
            level,
            factors,
        }
    }
}

fn factor(output: &mut Vec<PrivacyScoreFactor>, key: &'static str, points: i8) {
    output.push(PrivacyScoreFactor { key, points });
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RedactedPolicyExport {
    pub schema_version: u16,
    pub privacy_score: u8,
    pub strict_local_only: bool,
    pub encryption_at_rest: bool,
    pub telemetry_enabled: bool,
    pub sync_enabled: bool,
    pub retention_days: Option<u32>,
    pub denied_source_count: u32,
    pub capture_rule_count: u32,
    pub device_policy_count: u32,
}

impl RedactedPolicyExport {
    pub fn new(
        posture: PrivacyPostureInput,
        capture_rule_count: usize,
        device_policy_count: usize,
    ) -> Self {
        Self {
            schema_version: 1,
            privacy_score: PrivacyScore::calculate(posture).value,
            strict_local_only: posture.strict_local_only,
            encryption_at_rest: posture.encryption_at_rest,
            telemetry_enabled: posture.telemetry_enabled,
            sync_enabled: posture.sync_enabled,
            retention_days: posture.retention_days,
            denied_source_count: posture.denied_source_count,
            capture_rule_count: capture_rule_count.min(u32::MAX as usize) as u32,
            device_policy_count: device_policy_count.min(u32::MAX as usize) as u32,
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuleCounterView {
    pub rule_id: String,
    pub count: u64,
}

#[derive(Clone, Default)]
pub struct RuleAuditCounters {
    counters: BTreeMap<String, u64>,
}

impl fmt::Debug for RuleAuditCounters {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuleAuditCounters")
            .field("rule_count", &self.counters.len())
            .finish()
    }
}

impl RuleAuditCounters {
    pub fn increment(&mut self, rule_id: &str, count: u64) -> bool {
        if count == 0 || !valid_rule_id(rule_id) {
            return false;
        }
        if !self.counters.contains_key(rule_id) && self.counters.len() >= MAX_RULE_COUNTERS {
            return false;
        }
        let current = self.counters.entry(rule_id.to_owned()).or_default();
        *current = current.saturating_add(count);
        true
    }

    pub fn snapshot(&self) -> Vec<RuleCounterView> {
        self.counters
            .iter()
            .map(|(rule_id, count)| RuleCounterView {
                rule_id: rule_id.clone(),
                count: *count,
            })
            .collect()
    }
}

fn valid_rule_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_RULE_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrashDiagnosticCounters {
    pub timestamp_ms: u64,
    pub captured: u64,
    pub intentionally_skipped: u64,
    pub lost: u64,
    pub storage_errors: u64,
    pub permission_failures: u64,
}

#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct SealedCrashDiagnostics {
    version: u16,
    nonce: [u8; 24],
    ciphertext: Vec<u8>,
}

impl fmt::Debug for SealedCrashDiagnostics {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SealedCrashDiagnostics")
            .field("version", &self.version)
            .field("nonce", &"[redacted]")
            .field("ciphertext_bytes", &self.ciphertext.len())
            .finish()
    }
}

impl SealedCrashDiagnostics {
    pub fn seal(
        counters: CrashDiagnosticCounters,
        key: &[u8; 32],
        nonce: [u8; 24],
    ) -> Option<Self> {
        if *key == [0; 32] || nonce == [0; 24] {
            return None;
        }
        let plaintext = serde_json::to_vec(&counters).ok()?;
        let ciphertext = XChaCha20Poly1305::new(key.into())
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &plaintext,
                    aad: CRASH_AAD,
                },
            )
            .ok()?;
        (ciphertext.len() <= MAX_CRASH_CIPHERTEXT_BYTES).then_some(Self {
            version: 1,
            nonce,
            ciphertext,
        })
    }

    pub fn open(&self, key: &[u8; 32]) -> Option<CrashDiagnosticCounters> {
        if self.version != 1
            || *key == [0; 32]
            || self.ciphertext.len() < 16
            || self.ciphertext.len() > MAX_CRASH_CIPHERTEXT_BYTES
        {
            return None;
        }
        let plaintext = XChaCha20Poly1305::new(key.into())
            .decrypt(
                &XNonce::from(self.nonce),
                Payload {
                    msg: &self.ciphertext,
                    aad: CRASH_AAD,
                },
            )
            .ok()?;
        serde_json::from_slice(&plaintext).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privacy_score_has_bounded_explainable_factors() {
        let posture = PrivacyPostureInput {
            encryption_at_rest: true,
            strict_local_only: true,
            sensitive_memory_only: true,
            telemetry_enabled: false,
            sync_enabled: false,
            denied_source_count: 2,
            retention_days: Some(7),
        };
        let score = PrivacyScore::calculate(posture);
        assert_eq!(score.value, 100);
        assert_eq!(score.level, PrivacyScoreLevel::Strong);
        assert_eq!(score.factors.len(), 7);
        let export = RedactedPolicyExport::new(posture, 4, 2).to_json().unwrap();
        assert!(export.contains("\"schema_version\": 1"));
        assert!(!export.contains("source_app"));
    }

    #[test]
    fn audit_counters_saturate_without_clip_content() {
        let mut counters = RuleAuditCounters::default();
        assert!(counters.increment("skip.passwords", 2));
        assert!(counters.increment("skip.passwords", u64::MAX));
        assert_eq!(counters.snapshot()[0].count, u64::MAX);
        assert!(!counters.increment("bad rule", 1));
        assert!(!format!("{counters:?}").contains("passwords"));
    }

    #[test]
    fn crash_counters_are_authenticated_and_encrypted() {
        let counters = CrashDiagnosticCounters {
            timestamp_ms: 100,
            captured: 7,
            intentionally_skipped: 2,
            lost: 1,
            storage_errors: 1,
            permission_failures: 0,
        };
        let sealed = SealedCrashDiagnostics::seal(counters, &[9; 32], [4; 24]).unwrap();
        assert_eq!(sealed.open(&[9; 32]), Some(counters));
        assert_eq!(sealed.open(&[8; 32]), None);
        let serialized = serde_json::to_string(&sealed).unwrap();
        assert!(!serialized.contains("captured"));
        assert!(!format!("{sealed:?}").contains("100"));
    }
}
