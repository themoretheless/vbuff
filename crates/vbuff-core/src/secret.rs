//! Structural and entropy-based secret detection without retaining matches.
//!
//! This module is the single home of the secret domain. Three questions
//! belong here and nowhere else:
//!
//! 1. *What counts as a secret* - every detector, including the one-time
//!    password rules that used to live a second time in the capture gate.
//! 2. *How confident the detector is* - [`SecretFinding::confidence`].
//! 3. *How confident it has to be before something may act on the finding* -
//!    [`min_actionable_confidence`] and [`MIN_RECLASSIFY_CONFIDENCE`].
//!
//! Callers must not invent their own confidence literals; a bare `>= 0.9` in
//! a consumer is how the capture gate and the store came to disagree about
//! whether the same clipboard text was a secret.

/// A class of secret, ordered from most to least severe.
///
/// The declaration order is load-bearing: it is the tie-break when two
/// detectors fire with equal confidence, so the more severe class wins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecretKind {
    PrivateKey,
    CloudCredential,
    AccessToken,
    JsonWebToken,
    PaymentCard,
    OneTimePassword,
    RecoveryCode,
    HighEntropy,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SecretFinding {
    pub kind: SecretKind,
    pub confidence: f32,
}

impl SecretFinding {
    /// True when the evidence is strong enough to drive a capture-time
    /// decision: masking the clip, refusing sync/AI, bounding its TTL.
    #[must_use]
    pub fn is_actionable(self) -> bool {
        self.confidence >= min_actionable_confidence(self.kind)
    }

    /// True when the evidence is strong enough to retroactively downgrade a
    /// clip that is *already stored*. See [`MIN_RECLASSIFY_CONFIDENCE`] for
    /// why this is a separate, stricter question.
    ///
    /// Confidence is necessary but not sufficient: the evidence must also be
    /// structural rather than lexical (see [`SecretKind::is_structural`]).
    #[must_use]
    pub fn justifies_reclassification(self) -> bool {
        self.kind.is_structural() && self.confidence >= MIN_RECLASSIFY_CONFIDENCE
    }
}

impl SecretKind {
    /// Whether the evidence for this kind is the *shape of the value itself*
    /// rather than words that happen to surround it.
    ///
    /// A private key header, an `AKIA` prefix, a Luhn-valid card number or a
    /// JWT's three base64 segments do not occur by accident. A one-time code
    /// or a recovery code, by contrast, is recognised from an English marker
    /// near a digit run - and `error code 1024`, `discount code SAVE20` and
    /// `area code 415` are exactly that shape while being ordinary notes.
    ///
    /// Capture may act on either: masking a fresh copy is cheap and the user
    /// can re-copy. Retroactive reclassification may not, because it empties
    /// the searchable text of a clip the user has been living with, deletes
    /// its projections and forces a delete timer, with no undo.
    /// Whether a finding of this kind with `confidence` may drive retroactive
    /// reclassification. Test helper for the invariant; production code asks
    /// [`SecretFinding::justifies_reclassification`].
    #[cfg(test)]
    #[must_use]
    pub fn justifies_reclassification_at(self, confidence: f32) -> bool {
        SecretFinding {
            kind: self,
            confidence,
        }
        .justifies_reclassification()
    }

    #[must_use]
    pub const fn is_structural(self) -> bool {
        match self {
            Self::PrivateKey
            | Self::CloudCredential
            | Self::AccessToken
            | Self::JsonWebToken
            | Self::PaymentCard
            | Self::HighEntropy => true,
            Self::OneTimePassword | Self::RecoveryCode => false,
        }
    }
}

/// Confidence floor for acting on a finding at capture time, per kind.
///
/// This is the *capture* floor: the clip has not been stored yet, so acting
/// on a false positive costs a masked clip with a short TTL, while failing to
/// act costs a leaked secret. The entropy detector is therefore deliberately
/// allowed to act on weaker evidence than the structural detectors.
///
/// Every detector in this module currently emits a confidence at or above its
/// own floor, so the floor filters nothing today. That is intentional: it is a
/// contract, not a tuning knob. A future detector that emits weaker evidence
/// becomes inert instead of silently widening the sensitive lane, and
/// `every_detector_clears_its_own_actionable_floor` fails loudly if an
/// existing detector is ever tuned below its floor by accident.
#[must_use]
pub const fn min_actionable_confidence(kind: SecretKind) -> f32 {
    match kind {
        SecretKind::HighEntropy => 0.70,
        _ => 0.90,
    }
}

/// Confidence floor for retroactively reclassifying an *already stored* clip
/// as sensitive (the store's secret clawback scan).
///
/// Deliberately **not** the same question as [`min_actionable_confidence`],
/// and deliberately kind-independent:
///
/// * Capture asks "may I let this through unmasked?" about a clip nobody has
///   seen yet. Guessing wrong is cheap and reversible.
/// * Reclassification asks "may I scrub facets and embeddings off a clip the
///   user has been living with, and force an expiry onto it?" That is
///   destructive and retroactive, so it refuses the weakest detector
///   (`HighEntropy`, 0.72) even though capture acts on it.
///
/// The invariant is one-directional and pinned by
/// `reclassification_floor_is_never_looser_than_capture`: this constant is
/// never *below* any per-kind capture floor, so nothing can be clawed back
/// that capture itself would have shrugged at.
pub const MIN_RECLASSIFY_CONFIDENCE: f32 = 0.90;

/// Findings strong enough to act on at capture time.
#[must_use]
pub fn actionable_secrets(text: &str) -> Vec<SecretFinding> {
    let mut findings = detect_secrets(text);
    findings.retain(|finding| finding.is_actionable());
    findings
}

/// The finding a caller should *name* when several detectors fired: highest
/// confidence, with the more severe [`SecretKind`] breaking ties.
///
/// Naming is the only thing this resolves. Safety limits (TTL, memory-only)
/// must be taken from *every* finding, not just this one, or a clip that
/// looks like both an access token and a one-time password inherits the more
/// permissive handling of the two.
#[must_use]
pub fn strongest_finding(
    findings: impl IntoIterator<Item = SecretFinding>,
) -> Option<SecretFinding> {
    findings.into_iter().max_by(|left, right| {
        left.confidence
            .total_cmp(&right.confidence)
            .then_with(|| right.kind.cmp(&left.kind))
    })
}

/// Strongest actionable finding in `text`, if any.
#[must_use]
pub fn strongest_actionable_secret(text: &str) -> Option<SecretFinding> {
    strongest_finding(actionable_secrets(text))
}

/// Whether `text` carries evidence strong enough to warrant sensitive
/// handling. This is the single content-only sensitivity predicate; the
/// capture gate, edited-text re-checks and recall all ask it the same way.
#[must_use]
pub fn text_contains_actionable_secret(text: &str) -> bool {
    detect_secrets(text)
        .into_iter()
        .any(SecretFinding::is_actionable)
}

/// Every detector, at most one finding per [`SecretKind`], with no matched
/// text retained.
///
/// The findings are unfiltered: a caller that intends to *act* on one must
/// first pick a floor - [`actionable_secrets`] for capture-time decisions,
/// [`SecretFinding::justifies_reclassification`] for retroactive ones.
#[must_use]
pub fn detect_secrets(text: &str) -> Vec<SecretFinding> {
    let mut findings = Vec::new();
    if text.contains("-----BEGIN ") && text.contains("PRIVATE KEY-----") {
        findings.push(SecretFinding {
            kind: SecretKind::PrivateKey,
            confidence: 1.0,
        });
    }

    let lower = text.to_ascii_lowercase();
    let recovery_context = lower.contains("recovery") || lower.contains("backup code");
    if let Some(finding) = otp_finding(text, &lower) {
        findings.push(finding);
    }

    for token in text.split(|ch: char| ch.is_ascii_whitespace() || ",;()[]{}<>\"'".contains(ch)) {
        scan_token(token, recovery_context, &mut findings);
        // A credential pasted inside a URL - an OAuth callback, an API call
        // with the key in the query string - is one whitespace token, so the
        // structural detectors never saw it. Re-scan the locator's parts so
        // they do. Both granularities contribute, so nothing that matched
        // before can stop matching.
        if is_structured_locator(token) {
            for part in token.split(|ch: char| "/?&=#\\".contains(ch)) {
                scan_token(part, recovery_context, &mut findings);
            }
        }
    }
    findings
}

fn scan_token(token: &str, recovery_context: bool, findings: &mut Vec<SecretFinding>) {
    if is_cloud_credential(token) {
        push_once(findings, SecretKind::CloudCredential, 0.98);
    }
    if is_access_token(token) {
        push_once(findings, SecretKind::AccessToken, 0.97);
    }
    if is_jwt(token) {
        push_once(findings, SecretKind::JsonWebToken, 0.95);
    }
    let digits: String = token.chars().filter(char::is_ascii_digit).collect();
    if (13..=19).contains(&digits.len()) && luhn_valid(&digits) {
        push_once(findings, SecretKind::PaymentCard, 0.9);
    }
    if recovery_context && is_recovery_code(token) {
        push_once(findings, SecretKind::RecoveryCode, 0.94);
    }
    if probable_high_entropy(token) {
        push_once(findings, SecretKind::HighEntropy, 0.72);
    }
}

/// Whether a token is a URL or a filesystem path rather than an opaque blob.
///
/// Links and file paths are among the most-copied things there are, and both
/// clear the entropy bar easily: mixed case, digits and punctuation, spread
/// over enough characters. Scoring them as secrets marked ordinary clipboard
/// content sensitive, which withdraws it from sync and AI and puts a
/// two-minute delete timer on a clip the user copied deliberately.
///
/// The test is deliberately narrow. `://` and `\` appear in no credential
/// encoding, and a leading `/` marks an absolute path; base64 may contain `/`
/// but does not begin a token with one often enough to matter, and even then
/// only the weakest detector abstains while every structural one still runs
/// over the token and its parts.
fn is_structured_locator(token: &str) -> bool {
    token.contains("://") || token.contains('\\') || token.starts_with('/')
}

/// Context words that make a nearby digit run read as a one-time code.
///
/// This is the union of the two lists that used to disagree: the detector's
/// `["otp", "one-time", "verification code", "security code"]` and the capture
/// gate's `["code", "otp", "verification", "verify", "passcode"]`. `"code"`
/// subsumes `"verification code"`, `"security code"` and `"passcode"`;
/// `"one-time"` and `"verification"` are the two the other list lacked.
const OTP_CONTEXT_MARKERS: [&str; 5] = ["otp", "one-time", "verification", "verify", "code"];

/// Digit-run length that reads as a one-time code. The detector used to
/// require 6..=8 and the capture gate 4..=8; the wider bound wins, otherwise
/// a stored four-digit code would stay unclassified while an identical fresh
/// copy was masked.
const OTP_DIGIT_LENGTHS: std::ops::RangeInclusive<usize> = 4..=8;

/// A digit run backed by an explicit context word.
const OTP_CONTEXT_CONFIDENCE: f32 = 0.96;

/// A clip whose entire content is a short digit run, with no context word.
/// Weaker evidence than a context-backed match, but still actionable: this is
/// the shape of every code pasted straight out of an authenticator app, and
/// the capture gate has always treated it as a secret.
const OTP_BARE_CONFIDENCE: f32 = 0.90;

/// One-time password detection, unified from the two heuristics that used to
/// classify the same clipboard text differently depending on which code path
/// reached it first.
///
/// Two rules, deliberately a union rather than an intersection:
///
/// * context: any [`OTP_CONTEXT_MARKERS`] word anywhere in the text plus a
///   digit run of [`OTP_DIGIT_LENGTHS`]. The run is matched against maximal
///   digit sequences rather than whitespace-delimited tokens, so
///   `"verification code:123456"` is caught - punctuation used to hide it.
/// * bare: the whole trimmed text is nothing but a short digit run.
///
/// The bare rule is anchored to the *whole* text on purpose. Applying it per
/// token would make every four-digit number in ordinary prose - a year, a
/// street number, an invoice line - a secret.
fn otp_finding(text: &str, lower: &str) -> Option<SecretFinding> {
    let has_context = OTP_CONTEXT_MARKERS
        .iter()
        .any(|marker| lower.contains(marker));
    if has_context && has_otp_digit_run(text) {
        return Some(SecretFinding {
            kind: SecretKind::OneTimePassword,
            confidence: OTP_CONTEXT_CONFIDENCE,
        });
    }

    let trimmed = text.trim();
    let is_bare_code = OTP_DIGIT_LENGTHS.contains(&trimmed.len())
        && trimmed.bytes().all(|byte| byte.is_ascii_digit());
    is_bare_code.then_some(SecretFinding {
        kind: SecretKind::OneTimePassword,
        confidence: OTP_BARE_CONFIDENCE,
    })
}

fn has_otp_digit_run(text: &str) -> bool {
    text.split(|ch: char| !ch.is_ascii_digit())
        .any(|run| OTP_DIGIT_LENGTHS.contains(&run.len()))
}

fn is_recovery_code(token: &str) -> bool {
    let compact_len = token
        .bytes()
        .filter(|byte| byte.is_ascii_alphanumeric())
        .count();
    (8..=64).contains(&compact_len)
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && token.bytes().any(|byte| byte.is_ascii_digit())
        && token.bytes().any(|byte| byte.is_ascii_alphabetic())
}

fn push_once(findings: &mut Vec<SecretFinding>, kind: SecretKind, confidence: f32) {
    if !findings.iter().any(|finding| finding.kind == kind) {
        findings.push(SecretFinding { kind, confidence });
    }
}

fn is_cloud_credential(token: &str) -> bool {
    let bytes = token.as_bytes();
    bytes.len() == 20
        && (bytes.starts_with(b"AKIA") || bytes.starts_with(b"ASIA"))
        && token
            .bytes()
            .skip(4)
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

fn is_access_token(token: &str) -> bool {
    const PREFIXES: [&str; 5] = ["ghp_", "gho_", "github_pat_", "glpat-", "sk_live_"];
    PREFIXES
        .iter()
        .any(|prefix| token.starts_with(prefix) && token.len() >= prefix.len() + 16)
}

fn is_jwt(token: &str) -> bool {
    let mut parts = token.split('.');
    let Some(header) = parts.next() else {
        return false;
    };
    let Some(payload) = parts.next() else {
        return false;
    };
    let Some(signature) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && header.starts_with("eyJ")
        && payload.len() >= 8
        && signature.len() >= 16
        && [header, payload, signature]
            .into_iter()
            .all(|part| part.bytes().all(is_base64_url_byte))
}

fn is_base64_url_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'=')
}

fn probable_high_entropy(token: &str) -> bool {
    if !(24..=256).contains(&token.len()) || !token.is_ascii() {
        return false;
    }
    // Structure, not randomness: see [`is_structured_locator`]. The locator's
    // own parts are scanned separately, so a genuinely random query value or
    // path segment is still weighed on its own.
    if is_structured_locator(token) {
        return false;
    }
    let categories = [
        token.bytes().any(|byte| byte.is_ascii_lowercase()),
        token.bytes().any(|byte| byte.is_ascii_uppercase()),
        token.bytes().any(|byte| byte.is_ascii_digit()),
        token.bytes().any(|byte| !byte.is_ascii_alphanumeric()),
    ];
    categories.into_iter().filter(|present| *present).count() >= 3 && shannon_entropy(token) >= 3.8
}

fn shannon_entropy(value: &str) -> f64 {
    let mut counts = [0_u16; 128];
    for byte in value.bytes() {
        counts[usize::from(byte)] = counts[usize::from(byte)].saturating_add(1);
    }
    let len = value.len() as f64;
    counts
        .into_iter()
        .filter(|count| *count > 0)
        .map(|count| {
            let probability = f64::from(count) / len;
            -probability * probability.log2()
        })
        .sum()
}

fn luhn_valid(digits: &str) -> bool {
    let sum = digits
        .bytes()
        .rev()
        .enumerate()
        .map(|(index, byte)| {
            let mut digit = u32::from(byte - b'0');
            if index % 2 == 1 {
                digit *= 2;
                if digit > 9 {
                    digit -= 9;
                }
            }
            digit
        })
        .sum::<u32>();
    sum % 10 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_structural_tokens_without_returning_matched_text() {
        let findings = detect_secrets(
            "AKIAIOSFODNN7EXAMPLE ghp_abcdefghijklmnopqrstuvwxyz123456 4111111111111111",
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.kind == SecretKind::CloudCredential)
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.kind == SecretKind::AccessToken)
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.kind == SecretKind::PaymentCard)
        );
    }

    #[test]
    fn ordinary_prose_is_not_high_entropy() {
        assert!(detect_secrets("this is ordinary clipboard prose").is_empty());
    }

    /// Links and file paths are the most-copied content there is, and neither
    /// is a credential format. Scoring them on entropy alone marked them
    /// sensitive, which withdraws them from sync and AI and puts a short
    /// delete timer on a clip the user copied on purpose.
    #[test]
    fn links_and_file_paths_are_not_secrets_by_entropy_alone() {
        for ordinary in [
            "https://example.com/blog/post?utm_source=Newsletter&id=8fA2",
            "http://localhost:8080/api/v2/items?page=3&sort=createdAt",
            r"C:\Users\denis\Documents\Projects\report-2026-Q3.xlsx",
            "/Users/denis/Documents/Projects/report-2026-Q3.xlsx",
        ] {
            assert!(
                detect_secrets(ordinary).is_empty(),
                "{ordinary:?} was classified as a secret"
            );
        }
    }

    /// The exclusion above must not become a hiding place: a real credential
    /// is still found when it happens to sit inside a link.
    #[test]
    fn a_credential_inside_a_link_is_still_found() {
        let findings = detect_secrets(
            "https://example.com/callback?token=ghp_abcdefghijklmnopqrstuvwxyz123456",
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.kind == SecretKind::AccessToken),
            "{findings:?}"
        );
    }

    /// An opaque value carried by a link is weighed on its own, so excluding
    /// the locator does not exempt what it carries.
    #[test]
    fn an_opaque_value_inside_a_link_is_still_weighed() {
        let findings =
            detect_secrets("https://example.com/r?s=xQ7vR2pL9mK4wZ8tB6nH3jF5cD1aG0uY7eT4iS2");
        assert!(
            findings
                .iter()
                .any(|finding| finding.kind == SecretKind::HighEntropy),
            "{findings:?}"
        );
    }

    #[test]
    fn otp_and_recovery_codes_require_context() {
        assert!(
            detect_secrets("verification code 123456")
                .iter()
                .any(|finding| finding.kind == SecretKind::OneTimePassword)
        );
        assert!(
            detect_secrets("recovery code ABCD-1234-EFGH")
                .iter()
                .any(|finding| finding.kind == SecretKind::RecoveryCode)
        );
        assert!(
            !detect_secrets("invoice 123456")
                .iter()
                .any(|finding| finding.kind == SecretKind::OneTimePassword)
        );
    }

    fn otp_confidence(text: &str) -> Option<f32> {
        detect_secrets(text)
            .into_iter()
            .find(|finding| finding.kind == SecretKind::OneTimePassword)
            .map(|finding| finding.confidence)
    }

    /// The capture gate flagged a bare digit run as a one-time password; this
    /// detector required a context word, so the same clip was a secret while
    /// it was being copied and ordinary text once it was in the store.
    #[test]
    fn bare_digit_codes_are_detected_without_any_context_word() {
        for sample in ["123456", "4821", " 12345678 ", "0000"] {
            assert_eq!(
                otp_confidence(sample),
                Some(OTP_BARE_CONFIDENCE),
                "bare code {sample:?} must be a one-time password to every caller"
            );
        }
    }

    /// The bare rule is anchored to the whole clip: a short number sitting in
    /// prose is not a code, or every year and invoice number would be masked.
    #[test]
    fn bare_rule_does_not_turn_numbers_in_prose_into_codes() {
        for sample in [
            "meeting in 2026 at the office",
            "invoice 123456",
            "order 4821 shipped",
        ] {
            assert_eq!(otp_confidence(sample), None, "prose {sample:?}");
        }
    }

    /// Four-to-five digit codes and the capture gate's wider marker list were
    /// invisible to this detector: `"code"`, `"verify"` and `"verification"`
    /// were not markers here, and runs shorter than six digits were ignored.
    #[test]
    fn context_markers_and_short_runs_from_the_capture_gate_are_honored() {
        for sample in [
            "your login code is 4821",
            "verify with 12345",
            "verification 987654",
            "passcode 4821 expires soon",
        ] {
            assert_eq!(
                otp_confidence(sample),
                Some(OTP_CONTEXT_CONFIDENCE),
                "context sample {sample:?}"
            );
        }
    }

    /// The reverse direction: `"one-time"` was a marker here and nowhere in
    /// the capture gate's list, so this class was a secret to the store scan
    /// and ordinary text to the gate that decides whether to mask it.
    #[test]
    fn one_time_marker_is_kept_from_the_detector_side() {
        assert_eq!(
            otp_confidence("your one-time password is 483920"),
            Some(OTP_CONTEXT_CONFIDENCE)
        );
    }

    /// Splitting on whitespace and brackets meant a code glued to `:` or `=`
    /// was never isolated as a token; digit runs do not care about
    /// punctuation.
    #[test]
    fn codes_glued_to_punctuation_are_no_longer_hidden() {
        for sample in ["verification code:123456", "OTP=1234567", "code->48210"] {
            assert_eq!(
                otp_confidence(sample),
                Some(OTP_CONTEXT_CONFIDENCE),
                "punctuated sample {sample:?}"
            );
        }
    }

    /// Characterization, not endorsement: the merged rule inherits the
    /// capture gate's false positive, because narrowing it would have made
    /// this detector *weaker* than the gate that already ships it.
    #[test]
    fn merged_rule_inherits_the_capture_gates_marker_false_positive() {
        assert_eq!(
            otp_confidence("discount code SAVE20 valid until 2026"),
            Some(OTP_CONTEXT_CONFIDENCE)
        );
    }

    #[test]
    fn both_otp_rules_clear_the_actionable_and_reclassification_floors() {
        let floor = min_actionable_confidence(SecretKind::OneTimePassword);
        for confidence in [OTP_CONTEXT_CONFIDENCE, OTP_BARE_CONFIDENCE] {
            assert!(confidence >= floor, "{confidence} is inert at capture");
            assert!(
                confidence >= MIN_RECLASSIFY_CONFIDENCE,
                "{confidence} would be dropped by the store scan"
            );
        }
    }

    /// Sample per kind, so a detector that is retuned below its own floor
    /// fails here instead of silently going inert.
    fn sample_for(kind: SecretKind) -> &'static str {
        match kind {
            SecretKind::PrivateKey => "-----BEGIN OPENSSH PRIVATE KEY-----",
            SecretKind::CloudCredential => "AKIAIOSFODNN7EXAMPLE",
            SecretKind::AccessToken => "ghp_abcdefghijklmnopqrstuvwxyz123456",
            SecretKind::JsonWebToken => {
                "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.abcdefghijklmnop"
            }
            SecretKind::PaymentCard => "4111111111111111",
            SecretKind::OneTimePassword => "verification code 123456",
            SecretKind::RecoveryCode => "recovery code ABCD-1234-EFGH",
            SecretKind::HighEntropy => "fG7!qP2@vN9#xK4$mR8&zT5*",
        }
    }

    const ALL_KINDS: [SecretKind; 8] = [
        SecretKind::PrivateKey,
        SecretKind::CloudCredential,
        SecretKind::AccessToken,
        SecretKind::JsonWebToken,
        SecretKind::PaymentCard,
        SecretKind::OneTimePassword,
        SecretKind::RecoveryCode,
        SecretKind::HighEntropy,
    ];

    #[test]
    fn every_detector_clears_its_own_actionable_floor() {
        for kind in ALL_KINDS {
            let finding = detect_secrets(sample_for(kind))
                .into_iter()
                .find(|finding| finding.kind == kind)
                .unwrap_or_else(|| panic!("{kind:?} detector no longer fires on its sample"));
            assert!(
                finding.is_actionable(),
                "{kind:?} emits {} but needs {}",
                finding.confidence,
                min_actionable_confidence(kind)
            );
        }
    }

    #[test]
    fn reclassification_floor_is_never_looser_than_capture() {
        for kind in ALL_KINDS {
            assert!(
                MIN_RECLASSIFY_CONFIDENCE >= min_actionable_confidence(kind),
                "reclassifying {kind:?} would act on evidence capture ignores"
            );
        }
    }

    /// Exactly two things keep a kind out of retroactive reclassification:
    /// confidence below the floor (the entropy detector), and evidence that
    /// is lexical rather than structural (the code detectors, which fire on
    /// an English marker beside a digit run and so cannot tell "verification
    /// code 123456" from "error code 1024").
    #[test]
    fn reclassification_admits_only_confident_structural_evidence() {
        for kind in ALL_KINDS {
            let finding = detect_secrets(sample_for(kind))
                .into_iter()
                .find(|finding| finding.kind == kind)
                .expect("detector fires on its own sample");
            let expected = kind.is_structural() && finding.confidence >= MIN_RECLASSIFY_CONFIDENCE;
            assert_eq!(
                finding.justifies_reclassification(),
                expected,
                "{kind:?} disagrees between capture and reclassification"
            );
        }
        assert!(!SecretKind::HighEntropy.justifies_reclassification_at(0.72));
        assert!(!SecretKind::OneTimePassword.justifies_reclassification_at(0.99));
        assert!(SecretKind::PaymentCard.justifies_reclassification_at(0.90));
    }

    #[test]
    fn strongest_finding_prefers_confidence_then_severity() {
        let strongest = strongest_actionable_secret(
            "-----BEGIN OPENSSH PRIVATE KEY----- verification code 123456",
        )
        .expect("private key and one-time password both fire");
        assert_eq!(strongest.kind, SecretKind::PrivateKey);

        // Equal confidence: the earlier-declared, more severe kind wins.
        let tie = strongest_finding([
            SecretFinding {
                kind: SecretKind::HighEntropy,
                confidence: 0.9,
            },
            SecretFinding {
                kind: SecretKind::PaymentCard,
                confidence: 0.9,
            },
        ])
        .expect("non-empty");
        assert_eq!(tie.kind, SecretKind::PaymentCard);
    }
}
