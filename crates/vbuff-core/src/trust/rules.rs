use std::fmt;
use std::time::Duration;

use regex::Regex;
use thiserror::Error;

use crate::secret::{SecretKind, detect_secrets};

const MAX_RULES: usize = 256;
const MAX_RULE_ID_BYTES: usize = 96;
const MAX_SOURCE_BYTES: usize = 512;
const MAX_SAMPLE_BYTES: usize = 64 * 1_024;
const MAX_PATTERN_BYTES: usize = 1_024;
const MAX_PASTE_GUARD_AGE: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureRuleAction {
    Skip,
    CaptureNormally,
    CaptureEphemeral { ttl: Duration },
    Mask,
}

#[derive(Clone)]
enum CaptureMatcher {
    Source(String),
    Pattern(Regex),
}

#[derive(Clone)]
pub struct CaptureRule {
    id: String,
    matcher: CaptureMatcher,
    action: CaptureRuleAction,
}

impl fmt::Debug for CaptureRule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CaptureRule")
            .field("id", &self.id)
            .field(
                "matcher",
                &match self.matcher {
                    CaptureMatcher::Source(_) => "source:[redacted]",
                    CaptureMatcher::Pattern(_) => "pattern:[redacted]",
                },
            )
            .field("action", &self.action)
            .finish()
    }
}

impl CaptureRule {
    pub fn for_source(
        id: impl Into<String>,
        source_app: impl Into<String>,
        action: CaptureRuleAction,
    ) -> Result<Self, SimulationError> {
        let id = id.into();
        let source_app = source_app.into();
        validate_id(&id)?;
        validate_source(&source_app)?;
        validate_action(action)?;
        Ok(Self {
            id,
            matcher: CaptureMatcher::Source(source_app),
            action,
        })
    }

    pub fn for_pattern(
        id: impl Into<String>,
        pattern: &str,
        action: CaptureRuleAction,
    ) -> Result<Self, SimulationError> {
        let id = id.into();
        validate_id(&id)?;
        validate_action(action)?;
        if pattern.is_empty() || pattern.len() > MAX_PATTERN_BYTES {
            return Err(SimulationError::InvalidRule);
        }
        let matcher = Regex::new(pattern).map_err(|_| SimulationError::InvalidRule)?;
        Ok(Self {
            id,
            matcher: CaptureMatcher::Pattern(matcher),
            action,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Clone)]
pub struct SimulationInput<'a> {
    pub source_app: Option<&'a str>,
    pub sample: &'a str,
    pub os_sensitive_hint: bool,
}

impl fmt::Debug for SimulationInput<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SimulationInput")
            .field("has_source_app", &self.source_app.is_some())
            .field("sample_bytes", &self.sample.len())
            .field("os_sensitive_hint", &self.os_sensitive_hint)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleMatchReason {
    Source,
    Pattern,
    Secret(SecretKind),
    OperatingSystemHint,
    Default,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimulationResult {
    action: CaptureRuleAction,
    reason: RuleMatchReason,
    rule_id: Option<String>,
}

impl SimulationResult {
    pub const fn action(&self) -> CaptureRuleAction {
        self.action
    }

    pub const fn reason(&self) -> RuleMatchReason {
        self.reason
    }

    pub fn rule_id(&self) -> Option<&str> {
        self.rule_id.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum SimulationError {
    #[error("capture rule is invalid")]
    InvalidRule,
    #[error("simulation input is invalid")]
    InvalidInput,
    #[error("capture rule limit exceeded")]
    TooManyRules,
}

#[derive(Clone, Debug, Default)]
pub struct CaptureRuleSimulator {
    rules: Vec<CaptureRule>,
}

impl CaptureRuleSimulator {
    pub fn new(rules: impl IntoIterator<Item = CaptureRule>) -> Result<Self, SimulationError> {
        let rules = rules.into_iter().collect::<Vec<_>>();
        if rules.len() > MAX_RULES {
            return Err(SimulationError::TooManyRules);
        }
        let mut ids = rules
            .iter()
            .map(|rule| rule.id.as_str())
            .collect::<Vec<_>>();
        ids.sort_unstable();
        if ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(SimulationError::InvalidRule);
        }
        Ok(Self { rules })
    }

    pub fn simulate(
        &self,
        input: &SimulationInput<'_>,
    ) -> Result<SimulationResult, SimulationError> {
        if input.sample.len() > MAX_SAMPLE_BYTES
            || input
                .source_app
                .is_some_and(|source| validate_source(source).is_err())
        {
            return Err(SimulationError::InvalidInput);
        }
        let intrinsic_reason = intrinsic_sensitivity(input);
        for rule in &self.rules {
            let matched = match &rule.matcher {
                CaptureMatcher::Source(expected) => input.source_app == Some(expected.as_str()),
                CaptureMatcher::Pattern(pattern) => pattern.is_match(input.sample),
            };
            if matched {
                if rule.action == CaptureRuleAction::CaptureNormally
                    && let Some(reason) = intrinsic_reason
                {
                    return Ok(SimulationResult {
                        action: CaptureRuleAction::Mask,
                        reason,
                        rule_id: None,
                    });
                }
                return Ok(SimulationResult {
                    action: rule.action,
                    reason: match rule.matcher {
                        CaptureMatcher::Source(_) => RuleMatchReason::Source,
                        CaptureMatcher::Pattern(_) => RuleMatchReason::Pattern,
                    },
                    rule_id: Some(rule.id.clone()),
                });
            }
        }
        if let Some(reason) = intrinsic_reason {
            return Ok(SimulationResult {
                action: CaptureRuleAction::Mask,
                reason,
                rule_id: None,
            });
        }
        Ok(SimulationResult {
            action: CaptureRuleAction::CaptureNormally,
            reason: RuleMatchReason::Default,
            rule_id: None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteGuardDecision {
    Proceed,
    Quarantine,
    Expired,
    AlreadyConsumed,
}

#[derive(Clone, Debug)]
pub struct PasteGuard {
    selected_hash: [u8; 32],
    selected_at_ms: u64,
    maximum_age: Duration,
    consumed: bool,
}

impl PasteGuard {
    pub fn new(
        selected_hash: [u8; 32],
        selected_at_ms: u64,
        maximum_age: Duration,
    ) -> Option<Self> {
        (selected_hash != [0; 32] && !maximum_age.is_zero() && maximum_age <= MAX_PASTE_GUARD_AGE)
            .then_some(Self {
                selected_hash,
                selected_at_ms,
                maximum_age,
                consumed: false,
            })
    }

    pub fn verify(&mut self, observed_hash: [u8; 32], now_ms: u64) -> PasteGuardDecision {
        if self.consumed {
            return PasteGuardDecision::AlreadyConsumed;
        }
        self.consumed = true;
        let maximum_age_ms = self.maximum_age.as_millis().min(u128::from(u64::MAX)) as u64;
        if now_ms < self.selected_at_ms
            || now_ms.saturating_sub(self.selected_at_ms) > maximum_age_ms
        {
            return PasteGuardDecision::Expired;
        }
        if observed_hash == self.selected_hash {
            PasteGuardDecision::Proceed
        } else {
            PasteGuardDecision::Quarantine
        }
    }
}

fn intrinsic_sensitivity(input: &SimulationInput<'_>) -> Option<RuleMatchReason> {
    if input.os_sensitive_hint {
        return Some(RuleMatchReason::OperatingSystemHint);
    }
    detect_secrets(input.sample)
        .into_iter()
        .max_by(|left, right| left.confidence.total_cmp(&right.confidence))
        .map(|finding| RuleMatchReason::Secret(finding.kind))
}

fn validate_id(id: &str) -> Result<(), SimulationError> {
    if id.is_empty()
        || id.len() > MAX_RULE_ID_BYTES
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(SimulationError::InvalidRule);
    }
    Ok(())
}

fn validate_source(source: &str) -> Result<(), SimulationError> {
    if source.trim().is_empty()
        || source.len() > MAX_SOURCE_BYTES
        || source.chars().any(char::is_control)
    {
        return Err(SimulationError::InvalidInput);
    }
    Ok(())
}

fn validate_action(action: CaptureRuleAction) -> Result<(), SimulationError> {
    if matches!(action, CaptureRuleAction::CaptureEphemeral { ttl } if ttl.is_zero() || ttl > Duration::from_secs(24 * 60 * 60))
    {
        return Err(SimulationError::InvalidRule);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulator_returns_only_the_decision_and_rule_reason() {
        let simulator = CaptureRuleSimulator::new([
            CaptureRule::for_source(
                "passwords",
                "com.example.passwords",
                CaptureRuleAction::Skip,
            )
            .unwrap(),
            CaptureRule::for_pattern("ticket", r"TICKET-\d+", CaptureRuleAction::Mask).unwrap(),
        ])
        .unwrap();
        let input = SimulationInput {
            source_app: Some("editor"),
            sample: "TICKET-1234 secret body",
            os_sensitive_hint: false,
        };
        let result = simulator.simulate(&input).unwrap();
        assert_eq!(result.action(), CaptureRuleAction::Mask);
        assert_eq!(result.reason(), RuleMatchReason::Pattern);
        assert_eq!(result.rule_id(), Some("ticket"));
        assert!(!format!("{input:?}").contains("secret body"));
    }

    #[test]
    fn simulator_fails_closed_for_hints_and_detected_secrets() {
        let simulator = CaptureRuleSimulator::default();
        let hinted = simulator
            .simulate(&SimulationInput {
                source_app: None,
                sample: "ordinary",
                os_sensitive_hint: true,
            })
            .unwrap();
        assert_eq!(hinted.action(), CaptureRuleAction::Mask);
        let otp = simulator
            .simulate(&SimulationInput {
                source_app: None,
                sample: "verification code 123456",
                os_sensitive_hint: false,
            })
            .unwrap();
        assert_eq!(
            otp.reason(),
            RuleMatchReason::Secret(SecretKind::OneTimePassword)
        );
    }

    #[test]
    fn paste_guard_is_one_shot_and_quarantines_replacement() {
        let mut guard = PasteGuard::new([1; 32], 1_000, Duration::from_secs(2)).unwrap();
        assert_eq!(guard.verify([2; 32], 1_500), PasteGuardDecision::Quarantine);
        assert_eq!(
            guard.verify([1; 32], 1_600),
            PasteGuardDecision::AlreadyConsumed
        );
    }
}
