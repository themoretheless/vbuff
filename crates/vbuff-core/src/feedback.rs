//! Content-free feedback reports and explicit issue-draft URLs.

use std::fmt;
use std::sync::OnceLock;

use regex::Regex;
use url::Url;

const MAX_REPORT_BYTES: usize = 12 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub struct FeedbackEnvironment {
    pub version: String,
    pub os: String,
    pub architecture: String,
    pub session: String,
    pub capabilities: Vec<(String, String)>,
}

impl fmt::Debug for FeedbackEnvironment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FeedbackEnvironment")
            .field("version", &sanitize(&self.version))
            .field("os", &sanitize(&self.os))
            .field("architecture", &sanitize(&self.architecture))
            .field("session", &sanitize(&self.session))
            .field("capability_count", &self.capabilities.len())
            .finish()
    }
}

impl FeedbackEnvironment {
    pub fn redacted_preview(&self) -> String {
        let mut lines = vec![
            "## vbuff environment".to_string(),
            String::new(),
            format!("- Version: `{}`", sanitize(&self.version)),
            format!("- OS: `{}`", sanitize(&self.os)),
            format!("- Architecture: `{}`", sanitize(&self.architecture)),
            format!("- Session: `{}`", sanitize(&self.session)),
            String::new(),
            "### Capability probe".to_string(),
        ];
        for (name, state) in self.capabilities.iter().take(64) {
            lines.push(format!("- `{}`: {}", sanitize(name), sanitize(state)));
        }
        lines.push(String::new());
        lines.push("No clipboard contents, window titles, URLs, file paths, or source-app identifiers are included.".into());
        let mut report = lines.join("\n");
        truncate_utf8(&mut report, MAX_REPORT_BYTES);
        report
    }

    pub fn github_issue_draft_url(&self, repository: &str, title: &str) -> Option<String> {
        if repository.is_empty()
            || !repository.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/')
            })
            || repository.matches('/').count() != 1
        {
            return None;
        }
        let mut url = Url::parse(&format!("https://github.com/{repository}/issues/new")).ok()?;
        url.query_pairs_mut()
            .append_pair("title", &sanitize(title))
            .append_pair("body", &self.redacted_preview());
        Some(url.to_string())
    }
}

fn sanitize(value: &str) -> String {
    let without_controls = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let unix_home = unix_home_pattern();
    let windows_home = windows_home_pattern();
    let email = email_pattern();
    let token = token_pattern();
    let value = unix_home.replace_all(&without_controls, "$1/[redacted]");
    let value = windows_home.replace_all(&value, "C:\\Users\\[redacted]");
    let value = email.replace_all(&value, "[redacted-email]");
    token.replace_all(&value, "[redacted-token]").into_owned()
}

fn unix_home_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?i)(/users|/home)/[^/\s]+").unwrap())
}

fn windows_home_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?i)[a-z]:\\users\\[^\\\s]+").unwrap())
}

fn email_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?i)\b[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}\b").unwrap())
}

fn token_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\b(?:gh[pousr]_|sk-|xox[baprs]-)[a-z0-9_\-]{8,}\b").unwrap()
    })
}

fn truncate_utf8(value: &mut String, max_bytes: usize) {
    if value.len() <= max_bytes {
        return;
    }
    let mut boundary = max_bytes;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_redacts_identifiers_and_opens_only_an_issue_draft() {
        let report = FeedbackEnvironment {
            version: "0.1.0".into(),
            os: "linux /home/alice ghp_abcdefghijklmnopqrstuvwxyz".into(),
            architecture: "x86_64".into(),
            session: "wayland alice@example.test".into(),
            capabilities: vec![("capture".into(), "degraded".into())],
        };
        let preview = report.redacted_preview();
        assert!(!preview.contains("alice"));
        assert!(!preview.contains("ghp_"));
        assert!(preview.contains("No clipboard contents"));
        assert!(!format!("{report:?}").contains("alice"));
        let url = report
            .github_issue_draft_url("vbuff/vbuff", "Capture report")
            .unwrap();
        assert!(url.starts_with("https://github.com/vbuff/vbuff/issues/new?"));
        assert!(
            report
                .github_issue_draft_url("vbuff/vbuff/extra", "x")
                .is_none()
        );
    }
}
