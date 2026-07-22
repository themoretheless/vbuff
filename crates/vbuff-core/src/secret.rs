//! Structural and entropy-based secret detection without retaining matches.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

pub fn detect_secrets(text: &str) -> Vec<SecretFinding> {
    let mut findings = Vec::new();
    if text.contains("-----BEGIN ") && text.contains("PRIVATE KEY-----") {
        findings.push(SecretFinding {
            kind: SecretKind::PrivateKey,
            confidence: 1.0,
        });
    }

    let lower = text.to_ascii_lowercase();
    let otp_context = ["otp", "one-time", "verification code", "security code"]
        .iter()
        .any(|marker| lower.contains(marker));
    let recovery_context = lower.contains("recovery") || lower.contains("backup code");

    for token in text.split(|ch: char| ch.is_ascii_whitespace() || ",;()[]{}<>\"'".contains(ch)) {
        if is_cloud_credential(token) {
            push_once(&mut findings, SecretKind::CloudCredential, 0.98);
        }
        if is_access_token(token) {
            push_once(&mut findings, SecretKind::AccessToken, 0.97);
        }
        if is_jwt(token) {
            push_once(&mut findings, SecretKind::JsonWebToken, 0.95);
        }
        let digits: String = token.chars().filter(char::is_ascii_digit).collect();
        if (13..=19).contains(&digits.len()) && luhn_valid(&digits) {
            push_once(&mut findings, SecretKind::PaymentCard, 0.9);
        }
        if otp_context
            && (6..=8).contains(&token.len())
            && token.bytes().all(|byte| byte.is_ascii_digit())
        {
            push_once(&mut findings, SecretKind::OneTimePassword, 0.96);
        }
        if recovery_context && is_recovery_code(token) {
            push_once(&mut findings, SecretKind::RecoveryCode, 0.94);
        }
        if probable_high_entropy(token) {
            push_once(&mut findings, SecretKind::HighEntropy, 0.72);
        }
    }
    findings
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
}
