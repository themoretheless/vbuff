use std::fmt;

use serde::{Deserialize, Serialize};
use url::Url;
use vbuff_core::workflow::clean_link;
use vbuff_types::ContentKind;

use super::IntegrationContractError;

const MAX_ORIGIN_BYTES: usize = 4 * 1_024;

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
    SkipInvalidSize,
    SkipOversize,
}

impl BrowserIngress {
    pub fn decide(&self, max_bytes: u64) -> BrowserIngressDecision {
        match self.private_tab {
            Some(true) => return BrowserIngressDecision::SkipPrivate,
            None => return BrowserIngressDecision::SkipUnknownPrivacy,
            Some(false) => {}
        }
        if self.byte_size == 0 || max_bytes == 0 {
            return BrowserIngressDecision::SkipInvalidSize;
        }
        if self.byte_size > max_bytes {
            return BrowserIngressDecision::SkipOversize;
        }
        if self.origin.is_empty() || self.origin.len() > MAX_ORIGIN_BYTES {
            return BrowserIngressDecision::SkipInvalidOrigin;
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

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedLinkMetadata {
    pub href: String,
    pub label_bytes: u16,
    pub nofollow: bool,
}

impl fmt::Debug for SelectedLinkMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SelectedLinkMetadata")
            .field(
                "href",
                &format_args!("[redacted; {} bytes]", self.href.len()),
            )
            .field("label_bytes", &self.label_bytes)
            .field("nofollow", &self.nofollow)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserSourceReport {
    pub ingress: BrowserIngress,
    pub selected_link: Option<SelectedLinkMetadata>,
}

impl fmt::Debug for BrowserSourceReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BrowserSourceReport")
            .field("ingress", &self.ingress)
            .field("has_selected_link", &self.selected_link.is_some())
            .finish()
    }
}

impl BrowserSourceReport {
    pub fn validate(&self, max_bytes: u64) -> Result<(), IntegrationContractError> {
        if self.ingress.decide(max_bytes) != BrowserIngressDecision::Allow {
            return Err(IntegrationContractError::InvalidField);
        }
        let Some(link) = &self.selected_link else {
            return Ok(());
        };
        if link.href.is_empty() || link.href.len() > 16 * 1024 || link.label_bytes > 4_096 {
            return Err(IntegrationContractError::InvalidField);
        }
        let page =
            Url::parse(&self.ingress.origin).map_err(|_| IntegrationContractError::InvalidField)?;
        let selected =
            Url::parse(&link.href).map_err(|_| IntegrationContractError::InvalidField)?;
        if !matches!(selected.scheme(), "http" | "https")
            || !selected.username().is_empty()
            || selected.password().is_some()
            || selected.host_str().is_none()
            || page.host_str().is_none()
        {
            return Err(IntegrationContractError::InvalidField);
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanLinkRequest {
    pub url: String,
    pub explicit_user_gesture: bool,
    pub private_tab: Option<bool>,
}

impl fmt::Debug for CleanLinkRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CleanLinkRequest")
            .field("url", &format_args!("[redacted; {} bytes]", self.url.len()))
            .field("explicit_user_gesture", &self.explicit_user_gesture)
            .field("private_tab", &self.private_tab)
            .finish()
    }
}

impl CleanLinkRequest {
    pub fn execute(&self) -> Result<String, IntegrationContractError> {
        if !self.explicit_user_gesture || self.private_tab != Some(false) {
            return Err(IntegrationContractError::InvalidField);
        }
        clean_link(&self.url).map_err(|_| IntegrationContractError::InvalidField)
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
        ingress.byte_size = 0;
        assert_eq!(ingress.decide(100), BrowserIngressDecision::SkipInvalidSize);
        ingress.byte_size = 10;
        ingress.origin = "https://user@example.test/path".into();
        assert_eq!(
            ingress.decide(100),
            BrowserIngressDecision::SkipInvalidOrigin
        );
        ingress.origin = "x".repeat(MAX_ORIGIN_BYTES + 1);
        assert_eq!(
            ingress.decide(100),
            BrowserIngressDecision::SkipInvalidOrigin
        );
        assert!(!format!("{ingress:?}").contains("example.test"));
    }

    #[test]
    fn source_report_and_clean_link_require_explicit_non_private_context() {
        let report = BrowserSourceReport {
            ingress: BrowserIngress {
                origin: "https://example.test/".into(),
                private_tab: Some(false),
                kind: ContentKind::Url,
                byte_size: 120,
            },
            selected_link: Some(SelectedLinkMetadata {
                href: "https://docs.example.test/page?utm_source=test".into(),
                label_bytes: 12,
                nofollow: false,
            }),
        };
        report.validate(1_024).unwrap();
        assert!(!format!("{report:?}").contains("docs.example"));

        let request = CleanLinkRequest {
            url: report.selected_link.unwrap().href,
            explicit_user_gesture: true,
            private_tab: Some(false),
        };
        assert_eq!(request.execute().unwrap(), "https://docs.example.test/page");
        assert!(!format!("{request:?}").contains("docs.example"));
        assert!(
            CleanLinkRequest {
                private_tab: Some(true),
                ..request
            }
            .execute()
            .is_err()
        );
    }
}
