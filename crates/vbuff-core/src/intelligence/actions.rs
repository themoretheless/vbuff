use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::OnceLock;

use regex::Regex;
use thiserror::Error;
use url::Url;
use vbuff_types::{ContentKind, Flavor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentAction {
    OpenUrl,
    PasteAsPlainText,
    DecodeJwt,
    FormatJson,
    ConvertColor,
    ReviewShellCommand,
    Paste,
}

pub fn classify_intent(kind: ContentKind, text: Option<&str>) -> IntentAction {
    let trimmed = text.unwrap_or_default().trim();
    if looks_like_jwt(trimmed) {
        return IntentAction::DecodeJwt;
    }
    if kind == ContentKind::Url && Url::parse(trimmed).is_ok() {
        return IntentAction::OpenUrl;
    }
    if kind == ContentKind::Color {
        return IntentAction::ConvertColor;
    }
    if (trimmed.starts_with('{') || trimmed.starts_with('[')) && serde_like_json_shape(trimmed) {
        return IntentAction::FormatJson;
    }
    if kind == ContentKind::Code && looks_like_shell(trimmed) {
        return IntentAction::ReviewShellCommand;
    }
    if matches!(kind, ContentKind::Html | ContentKind::Rtf) {
        return IntentAction::PasteAsPlainText;
    }
    IntentAction::Paste
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteDestination {
    Terminal,
    CodeEditor,
    MarkdownEditor,
    Chat,
    RichDocument,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmartPasteMode {
    Preserve,
    PlainText,
    StripMarkdownFence,
    AddMarkdownFence,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SmartPastePlan {
    pub mode: SmartPasteMode,
    pub output: String,
}

impl fmt::Debug for SmartPastePlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SmartPastePlan")
            .field("mode", &self.mode)
            .field(
                "output",
                &format_args!("[redacted; {} bytes]", self.output.len()),
            )
            .finish()
    }
}

pub fn plan_smart_paste(
    destination: PasteDestination,
    kind: ContentKind,
    text: &str,
    language: Option<&str>,
) -> SmartPastePlan {
    match destination {
        PasteDestination::Terminal | PasteDestination::Chat => SmartPastePlan {
            mode: SmartPasteMode::PlainText,
            output: strip_markdown_fence(text).unwrap_or(text).to_string(),
        },
        PasteDestination::CodeEditor => SmartPastePlan {
            mode: SmartPasteMode::StripMarkdownFence,
            output: strip_markdown_fence(text).unwrap_or(text).to_string(),
        },
        PasteDestination::MarkdownEditor if kind == ContentKind::Code => {
            let language = language.unwrap_or_default().trim();
            SmartPastePlan {
                mode: SmartPasteMode::AddMarkdownFence,
                output: format!(
                    "```{language}\n{}\n```",
                    strip_markdown_fence(text).unwrap_or(text).trim_end()
                ),
            }
        }
        PasteDestination::MarkdownEditor
        | PasteDestination::RichDocument
        | PasteDestination::Unknown => SmartPastePlan {
            mode: SmartPasteMode::Preserve,
            output: text.to_string(),
        },
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ClipExplanation {
    pub kind: &'static str,
    pub summary: String,
}

impl fmt::Debug for ClipExplanation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClipExplanation")
            .field("kind", &self.kind)
            .field(
                "summary",
                &format_args!("[redacted; {} bytes]", self.summary.len()),
            )
            .finish()
    }
}

pub fn explain_text(text: &str) -> ClipExplanation {
    let trimmed = text.trim();
    if looks_like_jwt(trimmed) {
        return ClipExplanation {
            kind: "jwt",
            summary: "JWT-shaped token with header, claims, and signature segments; contents were not sent or logged".into(),
        };
    }
    if trimmed.len().is_multiple_of(2)
        && trimmed.len() >= 8
        && trimmed.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return ClipExplanation {
            kind: "hex",
            summary: format!("Hex-encoded value representing {} bytes", trimmed.len() / 2),
        };
    }
    if let Ok(url) = Url::parse(trimmed) {
        return ClipExplanation {
            kind: "url",
            summary: format!(
                "{} URL for {}",
                url.scheme().to_uppercase(),
                url.host_str().unwrap_or("an opaque destination")
            ),
        };
    }
    let error_lines = trimmed
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("error") || lower.contains("failed") || lower.contains("panic")
        })
        .count();
    if error_lines > 0 {
        return ClipExplanation {
            kind: "log",
            summary: format!("Log-like text with {error_lines} error-related line(s)"),
        };
    }
    ClipExplanation {
        kind: "text",
        summary: format!(
            "Text containing {} characters across {} line(s)",
            trimmed.chars().count(),
            trimmed.lines().count()
        ),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PiiFinding {
    pub category: &'static str,
    pub confidence: f32,
}

pub trait PiiDetectorBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn detect(&self, text: &str) -> Vec<PiiFinding>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RulePiiDetector;

impl PiiDetectorBackend for RulePiiDetector {
    fn id(&self) -> &'static str {
        "local-rules-v1"
    }

    fn detect(&self, text: &str) -> Vec<PiiFinding> {
        static EMAIL: OnceLock<Regex> = OnceLock::new();
        static PHONE: OnceLock<Regex> = OnceLock::new();
        let mut findings = Vec::new();
        let email = EMAIL.get_or_init(|| {
            Regex::new(r"(?i)\b[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}\b").unwrap()
        });
        let phone =
            PHONE.get_or_init(|| Regex::new(r"(?x)\b(?:\+?[0-9][0-9 ()\-]{7,}[0-9])\b").unwrap());
        if email.is_match(text) {
            findings.push(PiiFinding {
                category: "email",
                confidence: 0.82,
            });
        }
        if phone.is_match(text) {
            findings.push(PiiFinding {
                category: "phone",
                confidence: 0.72,
            });
        }
        let lower = text.to_ascii_lowercase();
        if ["medical record", "patient id", "diagnosis:"]
            .iter()
            .any(|marker| lower.contains(marker))
        {
            findings.push(PiiFinding {
                category: "medical",
                confidence: 0.88,
            });
        }
        findings
    }
}

#[derive(Clone, Default)]
pub struct ActiveTagger {
    token_labels: BTreeMap<String, BTreeMap<String, i32>>,
}

impl fmt::Debug for ActiveTagger {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let associations = self.token_labels.values().map(BTreeMap::len).sum::<usize>();
        formatter
            .debug_struct("ActiveTagger")
            .field("learned_tokens", &self.token_labels.len())
            .field("label_associations", &associations)
            .finish()
    }
}

impl ActiveTagger {
    pub fn observe(&mut self, text: &str, tag: &str, accepted: bool) {
        if tag.trim().is_empty() || tag.len() > 64 {
            return;
        }
        for token in bounded_tokens(text).into_iter().take(64) {
            let labels = self.token_labels.entry(token).or_default();
            if labels.len() >= 32 && !labels.contains_key(tag) {
                continue;
            }
            let score = labels.entry(tag.to_string()).or_default();
            *score = score
                .saturating_add(if accepted { 1 } else { -1 })
                .clamp(-32, 32);
        }
        if self.token_labels.len() > 4_096 {
            self.token_labels
                .retain(|_, labels| labels.values().any(|score| *score > 0));
        }
    }

    pub fn suggest(&self, text: &str, limit: usize) -> Vec<(String, i32)> {
        let mut scores = BTreeMap::<String, i32>::new();
        for token in bounded_tokens(text).into_iter().take(64) {
            if let Some(labels) = self.token_labels.get(&token) {
                for (label, score) in labels {
                    *scores.entry(label.clone()).or_default() += score;
                }
            }
        }
        let mut scores = scores
            .into_iter()
            .filter(|(_, score)| *score > 0)
            .collect::<Vec<_>>();
        scores.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        scores.truncate(limit.min(32));
        scores
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CaptionError {
    #[error("captioning is disabled")]
    Disabled,
    #[error("image input exceeds the backend limit")]
    TooLarge,
    #[error("caption backend failed")]
    Backend,
}

pub trait CaptionBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn max_input_bytes(&self) -> usize;
    fn caption(&self, mime: &str, bytes: &[u8]) -> Result<String, CaptionError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StructuralIdentity {
    Text,
    Url,
    CryptoAddress,
    Image,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteGuardFingerprint {
    digest: [u8; 32],
    identity: StructuralIdentity,
    host_hash: Option<[u8; 32]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteGuardDecision {
    Allow,
    BlockUnreadable,
    BlockContentChanged,
    BlockAddressSubstitution,
    BlockDomainSubstitution,
}

impl PasteGuardFingerprint {
    pub fn from_flavors(flavors: &[Flavor]) -> Option<Self> {
        if let Some(text) = flavors.iter().find_map(Flavor::as_text) {
            let normalized = text.replace("\r\n", "\n");
            let trimmed = normalized.trim();
            let parsed_url = Url::parse(trimmed).ok();
            let identity = if looks_like_crypto_address(trimmed) {
                StructuralIdentity::CryptoAddress
            } else if parsed_url.is_some() {
                StructuralIdentity::Url
            } else {
                StructuralIdentity::Text
            };
            let host_hash = parsed_url
                .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
                .map(|host| *blake3::hash(host.as_bytes()).as_bytes());
            return Some(Self {
                digest: *blake3::hash(normalized.as_bytes()).as_bytes(),
                identity,
                host_hash,
            });
        }
        let image = flavors.iter().find(|flavor| flavor.is_image())?;
        let bytes = image.body.inline_bytes()?;
        Some(Self {
            digest: *blake3::hash(bytes).as_bytes(),
            identity: StructuralIdentity::Image,
            host_hash: None,
        })
    }

    pub fn compare(&self, observed: Option<&Self>) -> PasteGuardDecision {
        let Some(observed) = observed else {
            return PasteGuardDecision::BlockUnreadable;
        };
        if self.digest == observed.digest && self.identity == observed.identity {
            return PasteGuardDecision::Allow;
        }
        if self.identity == StructuralIdentity::CryptoAddress
            && observed.identity == StructuralIdentity::CryptoAddress
        {
            return PasteGuardDecision::BlockAddressSubstitution;
        }
        if self.identity == StructuralIdentity::Url
            && observed.identity == StructuralIdentity::Url
            && self.host_hash != observed.host_hash
        {
            return PasteGuardDecision::BlockDomainSubstitution;
        }
        PasteGuardDecision::BlockContentChanged
    }
}

fn strip_markdown_fence(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    let first_newline = trimmed.find('\n')?;
    if !trimmed.starts_with("```") || !trimmed.ends_with("```") || first_newline + 3 > trimmed.len()
    {
        return None;
    }
    trimmed
        .get(first_newline + 1..trimmed.len() - 3)
        .map(str::trim_end)
}

fn looks_like_jwt(text: &str) -> bool {
    let segments = text.split('.').collect::<Vec<_>>();
    segments.len() == 3
        && segments.iter().all(|segment| {
            segment.len() >= 2
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
}

fn looks_like_shell(text: &str) -> bool {
    [
        "sudo ", "rm ", "curl ", "wget ", "cargo ", "git ", "docker ", "kubectl ",
    ]
    .iter()
    .any(|prefix| text.starts_with(prefix))
}

fn looks_like_crypto_address(text: &str) -> bool {
    (text.len() == 42
        && text.starts_with("0x")
        && text[2..].bytes().all(|byte| byte.is_ascii_hexdigit()))
        || ((26..=62).contains(&text.len())
            && text.bytes().all(|byte| byte.is_ascii_alphanumeric())
            && !text.contains(['I', 'O', 'l']))
}

fn serde_like_json_shape(text: &str) -> bool {
    (text.starts_with('{') && text.ends_with('}')) || (text.starts_with('[') && text.ends_with(']'))
}

fn bounded_tokens(text: &str) -> BTreeSet<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.len() >= 3 && token.len() <= 48)
        .map(str::to_lowercase)
        .take(256)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_and_smart_paste_are_deterministic_and_local() {
        assert_eq!(
            classify_intent(ContentKind::Url, Some("https://example.test")),
            IntentAction::OpenUrl
        );
        assert_eq!(
            classify_intent(ContentKind::Code, Some("sudo echo ok")),
            IntentAction::ReviewShellCommand
        );
        let plan = plan_smart_paste(
            PasteDestination::CodeEditor,
            ContentKind::Code,
            "```rust\nfn main() {}\n```",
            Some("rust"),
        );
        assert_eq!(plan.output, "fn main() {}");
        assert!(!format!("{plan:?}").contains("fn main"));
    }

    #[test]
    fn explanations_and_pii_findings_do_not_leak_through_debug() {
        let explanation = explain_text("0a0b0c0d");
        assert_eq!(explanation.kind, "hex");
        assert!(!format!("{explanation:?}").contains("0a0b"));
        let findings = RulePiiDetector.detect("Email a@example.test; patient id 42");
        assert!(findings.iter().any(|finding| finding.category == "email"));
        assert!(findings.iter().any(|finding| finding.category == "medical"));
    }

    #[test]
    fn active_tagger_learns_only_from_explicit_feedback() {
        let mut tagger = ActiveTagger::default();
        assert!(tagger.suggest("rust clipboard", 3).is_empty());
        tagger.observe("rust clipboard sqlite", "development", true);
        tagger.observe("rust clipboard ui", "development", true);
        assert_eq!(tagger.suggest("rust clipboard", 1)[0].0, "development");
        let debug = format!("{tagger:?}");
        assert!(!debug.contains("clipboard"));
        assert!(!debug.contains("development"));
        tagger.observe("rust clipboard", "development", false);
    }

    #[test]
    fn paste_guard_blocks_address_and_domain_swaps() {
        let expected = PasteGuardFingerprint::from_flavors(&[Flavor::inline(
            "text/plain",
            b"0x1111111111111111111111111111111111111111".to_vec(),
        )])
        .unwrap();
        let swapped = PasteGuardFingerprint::from_flavors(&[Flavor::inline(
            "text/plain;charset=utf-8",
            b"0x2222222222222222222222222222222222222222".to_vec(),
        )])
        .unwrap();
        assert_eq!(
            expected.compare(Some(&swapped)),
            PasteGuardDecision::BlockAddressSubstitution
        );

        let url = PasteGuardFingerprint::from_flavors(&[Flavor::inline(
            "text/plain",
            b"https://example.test/a".to_vec(),
        )])
        .unwrap();
        let lookalike = PasteGuardFingerprint::from_flavors(&[Flavor::inline(
            "text/plain",
            b"https://examp1e.test/a".to_vec(),
        )])
        .unwrap();
        assert_eq!(
            url.compare(Some(&lookalike)),
            PasteGuardDecision::BlockDomainSubstitution
        );
    }
}
