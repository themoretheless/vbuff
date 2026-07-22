use std::fmt;
use std::path::Path;

use url::Url;
use vbuff_types::Clip;

const MAX_URL_BYTES: usize = 16 * 1_024;
const MAX_PATH_BYTES: usize = 32 * 1_024;
const MAX_APP_BYTES: usize = 512;

#[derive(Clone, PartialEq, Eq)]
pub enum FindSourceAction {
    OpenUrl(String),
    RevealFile(String),
    ActivateApplication(String),
}

impl fmt::Debug for FindSourceAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (kind, bytes) = match self {
            Self::OpenUrl(value) => ("open_url", value.len()),
            Self::RevealFile(value) => ("reveal_file", value.len()),
            Self::ActivateApplication(value) => ("activate_application", value.len()),
        };
        formatter
            .debug_struct("FindSourceAction")
            .field("kind", &kind)
            .field("target_bytes", &bytes)
            .finish()
    }
}

impl FindSourceAction {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::OpenUrl(_) => "open_url",
            Self::RevealFile(_) => "reveal_file",
            Self::ActivateApplication(_) => "activate_application",
        }
    }

    pub fn target(&self) -> &str {
        match self {
            Self::OpenUrl(value) | Self::RevealFile(value) | Self::ActivateApplication(value) => {
                value
            }
        }
    }
}

pub fn find_source_action(clip: &Clip) -> Option<FindSourceAction> {
    if let Some(raw) = clip.meta.provenance.source_url.as_deref()
        && valid_url(raw)
    {
        return Some(FindSourceAction::OpenUrl(raw.to_owned()));
    }
    if let Some(path) = clip.meta.provenance.document_path.as_deref()
        && valid_path(path)
    {
        return Some(FindSourceAction::RevealFile(path.to_owned()));
    }
    clip.meta
        .provenance
        .app_id
        .as_deref()
        .or(clip.meta.source_app.as_deref())
        .filter(|app| valid_app(app))
        .map(|app| FindSourceAction::ActivateApplication(app.to_owned()))
}

fn valid_url(raw: &str) -> bool {
    if raw.is_empty() || raw.len() > MAX_URL_BYTES {
        return false;
    }
    Url::parse(raw).ok().is_some_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

fn valid_path(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= MAX_PATH_BYTES
        && !raw.chars().any(char::is_control)
        && Path::new(raw).is_absolute()
}

fn valid_app(raw: &str) -> bool {
    !raw.trim().is_empty() && raw.len() <= MAX_APP_BYTES && !raw.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use vbuff_types::{ClipId, ClipMeta, ContentKind, Flavor};

    use super::*;

    #[test]
    fn source_action_prefers_safe_url_and_redacts_debug() {
        let mut meta = ClipMeta::now(ContentKind::Url, 1, Some("browser".into()));
        meta.provenance.source_url = Some("https://example.test/page".into());
        meta.provenance.document_path = Some("/tmp/source.txt".into());
        let clip = Clip {
            id: ClipId::new(),
            flavors: vec![Flavor::inline("text/plain", b"x".to_vec())],
            content_hash: [1; 32],
            meta,
            pinned: false,
            favorite: false,
        };
        let action = find_source_action(&clip).unwrap();
        assert_eq!(action.kind(), "open_url");
        assert_eq!(action.target(), "https://example.test/page");
        assert!(!format!("{action:?}").contains("example.test"));
    }

    #[test]
    fn credentialed_urls_and_relative_paths_are_rejected() {
        let mut meta = ClipMeta::now(ContentKind::Text, 1, None);
        meta.provenance.source_url = Some("https://user@example.test/".into());
        meta.provenance.document_path = Some("relative.txt".into());
        let clip = Clip {
            id: ClipId::new(),
            flavors: vec![],
            content_hash: [1; 32],
            meta,
            pinned: false,
            favorite: false,
        };
        assert!(find_source_action(&clip).is_none());
    }
}
