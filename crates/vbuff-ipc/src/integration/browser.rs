use std::fmt;

use serde::{Deserialize, Serialize};
use url::Url;
use vbuff_types::ContentKind;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserIngress {
    pub origin: String,
    pub private_tab: Option<bool>,
    pub kind: ContentKind,
    pub byte_size: u64,
}

impl fmt::Debug for BrowserIngress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BrowserIngress")
            .field(
                "origin",
                &format_args!("[redacted; {} bytes]", self.origin.len()),
            )
            .field("private_tab", &self.private_tab)
            .field("kind", &self.kind)
            .field("byte_size", &self.byte_size)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserIngressDecision {
    Allow,
    SkipPrivate,
    SkipUnknownPrivacy,
    SkipInvalidOrigin,
    SkipOversize,
}

impl BrowserIngress {
    pub fn decide(&self, max_bytes: u64) -> BrowserIngressDecision {
        match self.private_tab {
            Some(true) => return BrowserIngressDecision::SkipPrivate,
            None => return BrowserIngressDecision::SkipUnknownPrivacy,
            Some(false) => {}
        }
        if self.byte_size > max_bytes {
            return BrowserIngressDecision::SkipOversize;
        }
        let valid_origin = Url::parse(&self.origin).ok().is_some_and(|url| {
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none()
        });
        if valid_origin {
            BrowserIngressDecision::Allow
        } else {
            BrowserIngressDecision::SkipInvalidOrigin
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_and_unknown_browser_contexts_fail_closed() {
        let mut ingress = BrowserIngress {
            origin: "https://example.test/".into(),
            private_tab: None,
            kind: ContentKind::Text,
            byte_size: 10,
        };
        assert_eq!(
            ingress.decide(100),
            BrowserIngressDecision::SkipUnknownPrivacy
        );
        ingress.private_tab = Some(true);
        assert_eq!(ingress.decide(100), BrowserIngressDecision::SkipPrivate);
        ingress.private_tab = Some(false);
        assert_eq!(ingress.decide(100), BrowserIngressDecision::Allow);
        ingress.origin = "https://user@example.test/path".into();
        assert_eq!(
            ingress.decide(100),
            BrowserIngressDecision::SkipInvalidOrigin
        );
        assert!(!format!("{ingress:?}").contains("example.test"));
    }
}
