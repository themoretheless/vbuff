use std::time::Duration;

use regex::Regex;
use url::Url;
use vbuff_types::{CaptureProvenance, Flavor, SensitivityReason};

use crate::secret::{SecretKind, detect_secrets};
use crate::trust::{handling_for_secret, sensitivity_reason_for_secret};

/// Clipboard source being evaluated by the capture gate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionSource {
    #[default]
    Clipboard,
    Primary,
}

/// A policy action resolved before content is persisted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaptureAction {
    #[default]
    Capture,
    Skip,
    PlainTextOnly,
    StripImages,
    CaptureSensitive,
}

/// Whether a rejected event was intentional policy or evidence of loss.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropClass {
    Intentional,
    Loss,
}

/// Stable accounting vocabulary for every exit from the capture path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DropReason {
    Empty,
    WhitespaceOnly,
    ExcludedSource,
    SourceRule,
    Concealed,
    SelfWriteSuppressed,
    PrimaryWithoutIntent,
    NoRealizedFlavor,
    TornRead,
    GenerationGap,
    GenerationStale,
    OwnerContention,
    TruncatedFlavor,
    OverSizeCap,
    Backpressure,
    DebounceCollapsed,
    StoreFailure,
    MemoryOnlyUnsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureOutcome {
    Captured,
    Dropped(DropReason),
}

impl CaptureOutcome {
    pub fn metric_key(self) -> String {
        match self {
            Self::Captured => "captured".into(),
            Self::Dropped(reason) => format!("dropped:{}", reason.as_str()),
        }
    }
}

impl DropReason {
    pub const fn class(self) -> DropClass {
        match self {
            Self::Empty
            | Self::WhitespaceOnly
            | Self::ExcludedSource
            | Self::SourceRule
            | Self::Concealed
            | Self::SelfWriteSuppressed
            | Self::PrimaryWithoutIntent
            | Self::DebounceCollapsed => DropClass::Intentional,
            Self::NoRealizedFlavor
            | Self::TornRead
            | Self::GenerationGap
            | Self::GenerationStale
            | Self::OwnerContention
            | Self::TruncatedFlavor
            | Self::OverSizeCap
            | Self::Backpressure
            | Self::StoreFailure => DropClass::Loss,
            Self::MemoryOnlyUnsupported => DropClass::Intentional,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::WhitespaceOnly => "whitespace_only",
            Self::ExcludedSource => "excluded_source",
            Self::SourceRule => "source_rule",
            Self::Concealed => "concealed",
            Self::SelfWriteSuppressed => "self_write_suppressed",
            Self::PrimaryWithoutIntent => "primary_without_intent",
            Self::NoRealizedFlavor => "no_realized_flavor",
            Self::TornRead => "torn_read",
            Self::GenerationGap => "generation_gap",
            Self::GenerationStale => "generation_stale",
            Self::OwnerContention => "owner_contention",
            Self::TruncatedFlavor => "truncated_flavor",
            Self::OverSizeCap => "over_size_cap",
            Self::Backpressure => "backpressure",
            Self::DebounceCollapsed => "debounce_collapsed",
            Self::StoreFailure => "store_failure",
            Self::MemoryOnlyUnsupported => "memory_only_unsupported",
        }
    }

    pub const fn is_recoverable(self) -> bool {
        false
    }
}

/// Matchers are conjunctive: every populated field must match.
#[derive(Clone, Debug, Default)]
pub struct SourcePredicate {
    pub app_contains: Option<String>,
    pub title_regex: Option<Regex>,
    pub url_host_suffix: Option<String>,
}

impl SourcePredicate {
    pub fn try_new(
        app_contains: Option<String>,
        title_regex: Option<&str>,
        url_host_suffix: Option<String>,
    ) -> Result<Self, regex::Error> {
        Ok(Self {
            app_contains,
            title_regex: title_regex.map(Regex::new).transpose()?,
            url_host_suffix: url_host_suffix.map(|host| host.to_lowercase()),
        })
    }

    pub fn matches(&self, provenance: &CaptureProvenance) -> bool {
        let app_matches = self.app_contains.as_ref().is_none_or(|needle| {
            provenance
                .app_id
                .as_deref()
                .is_some_and(|app| app.to_lowercase().contains(&needle.to_lowercase()))
        });
        let title_matches = self.title_regex.as_ref().is_none_or(|pattern| {
            provenance
                .window_title
                .as_deref()
                .is_some_and(|title| pattern.is_match(title))
        });
        let host_matches = self.url_host_suffix.as_ref().is_none_or(|suffix| {
            provenance.source_url.as_deref().is_some_and(|raw| {
                Url::parse(raw)
                    .ok()
                    .and_then(|url| url.host_str().map(str::to_lowercase))
                    .is_some_and(|host| host == *suffix || host.ends_with(&format!(".{suffix}")))
            })
        });
        app_matches && title_matches && host_matches
    }
}

#[derive(Clone, Debug)]
pub struct CaptureRule {
    pub predicate: SourcePredicate,
    pub action: CaptureAction,
}

/// Inputs are borrowed so policy evaluation never copies clipboard bytes.
#[derive(Clone, Copy, Debug)]
pub struct CaptureInput<'a> {
    pub flavors: &'a [Flavor],
    pub provenance: &'a CaptureProvenance,
    pub source: SelectionSource,
    pub primary_intended: bool,
    pub coherent_generation: bool,
    pub concealed: bool,
    pub self_write: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureDecision {
    Capture {
        action: CaptureAction,
        sensitive: bool,
        sync_eligible: bool,
        ai_allowed: bool,
        memory_only: bool,
        expires_after: Option<Duration>,
        sensitivity_reason: Option<SensitivityReason>,
    },
    Skip(DropReason),
}

#[derive(Clone, Debug)]
pub struct CapturePolicy {
    pub skip_whitespace_only: bool,
    pub excluded_apps: Vec<String>,
    pub rules: Vec<CaptureRule>,
    pub otp_ttl: Duration,
    pub detect_secrets: bool,
    pub secret_ttl: Duration,
}

impl Default for CapturePolicy {
    fn default() -> Self {
        Self {
            skip_whitespace_only: true,
            excluded_apps: Vec::new(),
            rules: Vec::new(),
            otp_ttl: Duration::from_secs(90),
            detect_secrets: true,
            secret_ttl: Duration::from_secs(10 * 60),
        }
    }
}

impl CapturePolicy {
    pub fn decide(&self, input: CaptureInput<'_>) -> CaptureDecision {
        if input.flavors.is_empty() {
            return CaptureDecision::Skip(DropReason::Empty);
        }
        if !input.coherent_generation {
            return CaptureDecision::Skip(DropReason::TornRead);
        }
        if input.self_write {
            return CaptureDecision::Skip(DropReason::SelfWriteSuppressed);
        }
        if input.source == SelectionSource::Primary && !input.primary_intended {
            return CaptureDecision::Skip(DropReason::PrimaryWithoutIntent);
        }
        if input.concealed {
            return CaptureDecision::Skip(DropReason::Concealed);
        }
        if !input.flavors.iter().any(Flavor::is_realized) {
            return CaptureDecision::Skip(DropReason::NoRealizedFlavor);
        }

        let source_app = input.provenance.app_id.as_deref().unwrap_or_default();
        if self
            .excluded_apps
            .iter()
            .any(|excluded| source_app.to_lowercase().contains(&excluded.to_lowercase()))
        {
            return CaptureDecision::Skip(DropReason::ExcludedSource);
        }

        let action = self
            .rules
            .iter()
            .find(|rule| rule.predicate.matches(input.provenance))
            .map_or(CaptureAction::Capture, |rule| rule.action);
        if action == CaptureAction::Skip {
            return CaptureDecision::Skip(DropReason::SourceRule);
        }

        let has_non_text = input
            .flavors
            .iter()
            .any(|flavor| flavor.is_realized() && !flavor.is_text());
        let has_text = input
            .flavors
            .iter()
            .any(|flavor| flavor.is_realized() && flavor.as_text().is_some());
        let all_text_whitespace = input
            .flavors
            .iter()
            .filter(|flavor| flavor.is_realized())
            .filter_map(Flavor::as_text)
            .all(|value| value.trim().is_empty());
        if self.skip_whitespace_only && has_text && !has_non_text && all_text_whitespace {
            return CaptureDecision::Skip(DropReason::WhitespaceOnly);
        }

        let otp = input
            .flavors
            .iter()
            .filter(|flavor| flavor.is_realized())
            .filter_map(Flavor::as_text)
            .any(is_probable_otp);
        let secret_finding = self.detect_secrets.then(|| {
            input
                .flavors
                .iter()
                .filter(|flavor| flavor.is_realized())
                .filter_map(Flavor::as_text)
                .flat_map(detect_secrets)
                .filter(|finding| finding.confidence >= secret_threshold(finding.kind))
                .max_by(|left, right| left.confidence.total_cmp(&right.confidence))
        });
        let secret_kind = secret_finding.flatten().map(|finding| finding.kind);
        let forced_sensitive = action == CaptureAction::CaptureSensitive;
        let sensitivity_reason = if otp {
            Some(SensitivityReason::OneTimePassword)
        } else if let Some(kind) = secret_kind {
            Some(sensitivity_reason_for_secret(kind))
        } else if forced_sensitive {
            Some(SensitivityReason::CaptureRule)
        } else {
            None
        };
        CaptureDecision::Capture {
            action,
            sensitive: sensitivity_reason.is_some(),
            sync_eligible: sensitivity_reason.is_none(),
            ai_allowed: sensitivity_reason.is_none(),
            memory_only: otp
                || secret_kind.is_some_and(|kind| handling_for_secret(kind).memory_only),
            expires_after: if otp {
                Some(self.otp_ttl.min(self.secret_ttl))
            } else if let Some(kind) = secret_kind {
                let handling = handling_for_secret(kind);
                Some(handling.ttl.min(self.secret_ttl))
            } else {
                None
            },
            sensitivity_reason,
        }
    }
}

const fn secret_threshold(kind: SecretKind) -> f32 {
    match kind {
        SecretKind::HighEntropy => 0.7,
        _ => 0.9,
    }
}

fn is_probable_otp(text: &str) -> bool {
    let trimmed = text.trim();
    if (4..=8).contains(&trimmed.len()) && trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        return true;
    }

    let lower = trimmed.to_lowercase();
    let has_marker = ["code", "otp", "verification", "verify", "passcode"]
        .iter()
        .any(|marker| lower.contains(marker));
    has_marker
        && lower
            .split(|ch: char| !ch.is_ascii_digit())
            .any(|part| (4..=8).contains(&part.len()))
}

/// Apply the capture gate's content-only sensitivity rules to edited text.
pub fn text_requires_sensitive_handling(text: &str) -> bool {
    is_probable_otp(text)
        || detect_secrets(text)
            .into_iter()
            .any(|finding| finding.confidence >= secret_threshold(finding.kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_types::{CaptureProvenance, FlavorRealization};

    fn input<'a>(flavors: &'a [Flavor], provenance: &'a CaptureProvenance) -> CaptureInput<'a> {
        CaptureInput {
            flavors,
            provenance,
            source: SelectionSource::Clipboard,
            primary_intended: true,
            coherent_generation: true,
            concealed: false,
            self_write: false,
        }
    }

    #[test]
    fn otp_is_ephemeral_sensitive_and_never_synced() {
        let flavors = [Flavor::inline("text/plain", b"123456".to_vec())];
        let provenance = CaptureProvenance::default();
        assert_eq!(
            CapturePolicy::default().decide(input(&flavors, &provenance)),
            CaptureDecision::Capture {
                action: CaptureAction::Capture,
                sensitive: true,
                sync_eligible: false,
                ai_allowed: false,
                memory_only: true,
                expires_after: Some(Duration::from_secs(90)),
                sensitivity_reason: Some(SensitivityReason::OneTimePassword),
            }
        );
    }

    #[test]
    fn structural_secret_is_ephemeral_sensitive_and_never_synced() {
        let flavors = [Flavor::inline(
            "text/plain",
            b"ghp_abcdefghijklmnopqrstuvwxyz123456".to_vec(),
        )];
        let provenance = CaptureProvenance::default();
        assert_eq!(
            CapturePolicy::default().decide(input(&flavors, &provenance)),
            CaptureDecision::Capture {
                action: CaptureAction::Capture,
                sensitive: true,
                sync_eligible: false,
                ai_allowed: false,
                memory_only: false,
                expires_after: Some(Duration::from_secs(5 * 60)),
                sensitivity_reason: Some(SensitivityReason::AccessToken),
            }
        );
    }

    #[test]
    fn secret_kinds_receive_distinct_capture_ttls() {
        let provenance = CaptureProvenance::default();
        let card = [Flavor::inline("text/plain", b"4111111111111111".to_vec())];
        let private_key = [Flavor::inline(
            "text/plain",
            b"-----BEGIN OPENSSH PRIVATE KEY-----".to_vec(),
        )];
        assert!(matches!(
            CapturePolicy::default().decide(input(&card, &provenance)),
            CaptureDecision::Capture {
                expires_after: Some(ttl),
                ..
            } if ttl == Duration::from_secs(10 * 60)
        ));
        assert!(matches!(
            CapturePolicy::default().decide(input(&private_key, &provenance)),
            CaptureDecision::Capture {
                expires_after: Some(ttl),
                ..
            } if ttl == Duration::from_secs(30)
        ));
    }

    #[test]
    fn user_secret_ttl_is_a_hard_cap_for_every_detected_secret() {
        let policy = CapturePolicy {
            secret_ttl: Duration::from_secs(60),
            ..CapturePolicy::default()
        };
        let provenance = CaptureProvenance::default();
        let samples = [
            "123456",
            "ghp_abcdefghijklmnopqrstuvwxyz123456",
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.abcdefghijklmnop",
            "4111111111111111",
            "-----BEGIN OPENSSH PRIVATE KEY-----",
        ];

        for sample in samples {
            let flavors = [Flavor::inline("text/plain", sample.as_bytes().to_vec())];
            assert!(matches!(
                policy.decide(input(&flavors, &provenance)),
                CaptureDecision::Capture {
                    sensitive: true,
                    expires_after: Some(ttl),
                    ..
                } if ttl <= Duration::from_secs(60)
            ));
        }
    }

    #[test]
    fn every_text_flavor_is_scanned_and_non_text_prevents_whitespace_drop() {
        let provenance = CaptureProvenance::default();
        let secret_second = [
            Flavor::inline("text/plain", b"ordinary".to_vec()),
            Flavor::inline(
                "text/x-token",
                b"ghp_abcdefghijklmnopqrstuvwxyz123456".to_vec(),
            ),
        ];
        assert!(matches!(
            CapturePolicy::default().decide(input(&secret_second, &provenance)),
            CaptureDecision::Capture {
                sensitive: true,
                sync_eligible: false,
                ..
            }
        ));

        let image_with_empty_text = [
            Flavor::inline("text/plain", b"  ".to_vec()),
            Flavor::inline("image/png", vec![1, 2, 3]),
        ];
        assert!(matches!(
            CapturePolicy::default().decide(input(&image_with_empty_text, &provenance)),
            CaptureDecision::Capture { .. }
        ));
    }

    #[test]
    fn high_entropy_detector_uses_its_kind_specific_threshold() {
        let flavors = [Flavor::inline(
            "text/plain",
            b"fG7!qP2@vN9#xK4$mR8&zT5*".to_vec(),
        )];
        let provenance = CaptureProvenance::default();
        assert!(matches!(
            CapturePolicy::default().decide(input(&flavors, &provenance)),
            CaptureDecision::Capture {
                sensitive: true,
                memory_only: false,
                sensitivity_reason: Some(SensitivityReason::PossibleSecret),
                ..
            }
        ));
    }

    #[test]
    fn source_rules_match_app_title_and_url_host() {
        let policy = CapturePolicy {
            rules: vec![CaptureRule {
                predicate: SourcePredicate {
                    app_contains: Some("browser".into()),
                    title_regex: Some(Regex::new("(?i)bank").unwrap()),
                    url_host_suffix: Some("example.com".into()),
                },
                action: CaptureAction::CaptureSensitive,
            }],
            ..Default::default()
        };
        let flavors = [Flavor::inline("text/plain", b"balance".to_vec())];
        let provenance = CaptureProvenance {
            app_id: Some("org.browser.App".into()),
            window_title: Some("Bank account".into()),
            source_url: Some("https://secure.example.com/account".into()),
            ..Default::default()
        };

        assert_eq!(
            policy.decide(input(&flavors, &provenance)),
            CaptureDecision::Capture {
                action: CaptureAction::CaptureSensitive,
                sensitive: true,
                sync_eligible: false,
                ai_allowed: false,
                memory_only: false,
                expires_after: None,
                sensitivity_reason: Some(SensitivityReason::CaptureRule),
            }
        );
    }

    #[test]
    fn rejects_torn_primary_and_unrealized_inputs() {
        let mut flavors = [Flavor::inline("text/plain", b"hello".to_vec())];
        let provenance = CaptureProvenance::default();
        let policy = CapturePolicy::default();

        let mut candidate = input(&flavors, &provenance);
        candidate.coherent_generation = false;
        assert_eq!(
            policy.decide(candidate),
            CaptureDecision::Skip(DropReason::TornRead)
        );

        let mut candidate = input(&flavors, &provenance);
        candidate.source = SelectionSource::Primary;
        candidate.primary_intended = false;
        assert_eq!(
            policy.decide(candidate),
            CaptureDecision::Skip(DropReason::PrimaryWithoutIntent)
        );

        flavors[0].realization = FlavorRealization::Failed;
        assert_eq!(
            policy.decide(input(&flavors, &provenance)),
            CaptureDecision::Skip(DropReason::NoRealizedFlavor)
        );
    }
}
