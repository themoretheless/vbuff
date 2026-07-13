use std::time::Duration;

use regex::Regex;
use url::Url;
use vbuff_types::{CaptureProvenance, Flavor};

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
        }
    }

    pub const fn is_recoverable(self) -> bool {
        matches!(
            self,
            Self::Concealed | Self::ExcludedSource | Self::SourceRule
        )
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
        expires_after: Option<Duration>,
    },
    Skip(DropReason),
}

#[derive(Clone, Debug)]
pub struct CapturePolicy {
    pub skip_whitespace_only: bool,
    pub excluded_apps: Vec<String>,
    pub rules: Vec<CaptureRule>,
    pub otp_ttl: Duration,
}

impl Default for CapturePolicy {
    fn default() -> Self {
        Self {
            skip_whitespace_only: true,
            excluded_apps: Vec::new(),
            rules: Vec::new(),
            otp_ttl: Duration::from_secs(90),
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

        let text = input.flavors.iter().find_map(Flavor::as_text);
        if self.skip_whitespace_only && text.is_some_and(|value| value.trim().is_empty()) {
            return CaptureDecision::Skip(DropReason::WhitespaceOnly);
        }

        let otp = text.is_some_and(is_probable_otp);
        CaptureDecision::Capture {
            action,
            sensitive: otp || action == CaptureAction::CaptureSensitive,
            sync_eligible: !otp && action != CaptureAction::CaptureSensitive,
            expires_after: otp.then_some(self.otp_ttl),
        }
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
                expires_after: Some(Duration::from_secs(90)),
            }
        );
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
                expires_after: None,
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
