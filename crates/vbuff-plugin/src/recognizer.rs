use vbuff_types::ContentKind;

use crate::{PluginError, Result};

const MAX_RECOGNIZER_TEXT_BYTES: usize = 1024 * 1024;
const MAX_ACTION_CANDIDATES: usize = 32;

#[derive(Clone, Copy)]
pub struct RecognizerInput<'a> {
    kind: ContentKind,
    text: Option<&'a str>,
    sensitive: bool,
}

impl<'a> RecognizerInput<'a> {
    pub fn new(kind: ContentKind, text: Option<&'a str>, sensitive: bool) -> Result<Self> {
        if text.is_some_and(|text| text.len() > MAX_RECOGNIZER_TEXT_BYTES) {
            return Err(PluginError::InvalidInput(
                "recognizer text exceeds the byte limit".into(),
            ));
        }
        if sensitive && text.is_some() {
            return Err(PluginError::CapabilityDenied(
                "sensitive recognizer input is metadata-only".into(),
            ));
        }
        Ok(Self {
            kind,
            text,
            sensitive,
        })
    }

    pub const fn kind(self) -> ContentKind {
        self.kind
    }

    pub const fn text(self) -> Option<&'a str> {
        self.text
    }

    pub const fn sensitive(self) -> bool {
        self.sensitive
    }
}

impl std::fmt::Debug for RecognizerInput<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecognizerInput")
            .field("kind", &self.kind)
            .field("text_bytes", &self.text.map(str::len))
            .field("sensitive", &self.sensitive)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum TypedAction {
    OpenUrl { url: String },
    OpenFile { path: String },
    CopyDerivedText { text: String },
    ApplyPipeline { pipeline_id: String },
}

impl std::fmt::Debug for TypedAction {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenUrl { .. } => formatter.write_str("OpenUrl([redacted])"),
            Self::OpenFile { .. } => formatter.write_str("OpenFile([redacted])"),
            Self::CopyDerivedText { text } => formatter
                .debug_tuple("CopyDerivedText")
                .field(&format_args!("[redacted; {} bytes]", text.len()))
                .finish(),
            Self::ApplyPipeline { pipeline_id } => formatter
                .debug_struct("ApplyPipeline")
                .field("pipeline_id", pipeline_id)
                .finish(),
        }
    }
}

impl TypedAction {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::OpenUrl { url } => {
                if url.len() > 4_096 {
                    return Err(PluginError::InvalidInput("action URL is too long".into()));
                }
                let parsed = url::Url::parse(url)
                    .map_err(|_| PluginError::InvalidInput("invalid action URL".into()))?;
                if !matches!(parsed.scheme(), "http" | "https" | "mailto") {
                    return Err(PluginError::InvalidInput(
                        "action URL scheme is not allowed".into(),
                    ));
                }
            }
            Self::OpenFile { path } => {
                if path.is_empty() || path.len() > 4_096 || path.chars().any(char::is_control) {
                    return Err(PluginError::InvalidInput("invalid action path".into()));
                }
            }
            Self::CopyDerivedText { text } => {
                if text.len() > 1_048_576 {
                    return Err(PluginError::InvalidInput(
                        "derived action text is too large".into(),
                    ));
                }
            }
            Self::ApplyPipeline { pipeline_id } => {
                if pipeline_id.is_empty()
                    || pipeline_id.len() > 128
                    || !pipeline_id.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                    })
                {
                    return Err(PluginError::InvalidInput("pipeline id is empty".into()));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ActionCandidate {
    pub label: String,
    pub confidence: u8,
    pub action: TypedAction,
}

impl std::fmt::Debug for ActionCandidate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ActionCandidate")
            .field(
                "label",
                &format_args!("[redacted; {} bytes]", self.label.len()),
            )
            .field("confidence", &self.confidence)
            .field("action", &self.action)
            .finish()
    }
}

impl ActionCandidate {
    pub fn validate(&self) -> Result<()> {
        if self.label.trim().is_empty()
            || self.label.len() > 80
            || self.label.chars().any(char::is_control)
        {
            return Err(PluginError::InvalidInput("invalid action label".into()));
        }
        if self.confidence > 100 {
            return Err(PluginError::InvalidInput(
                "action confidence exceeds 100".into(),
            ));
        }
        self.action.validate()
    }
}

pub trait Recognizer: Send + Sync {
    fn id(&self) -> &'static str;
    fn recognize(&self, input: RecognizerInput<'_>) -> Result<Vec<ActionCandidate>>;
}

pub fn run_recognizer(
    recognizer: &dyn Recognizer,
    input: RecognizerInput<'_>,
) -> Result<Vec<ActionCandidate>> {
    let candidates = recognizer.recognize(input)?;
    if candidates.len() > MAX_ACTION_CANDIDATES {
        return Err(PluginError::InvalidInput(
            "recognizer returned too many actions".into(),
        ));
    }
    candidates.iter().try_for_each(ActionCandidate::validate)?;
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizer_actions_reject_active_content_schemes() {
        assert!(
            TypedAction::OpenUrl {
                url: "javascript:alert(1)".into()
            }
            .validate()
            .is_err()
        );
        assert!(
            TypedAction::OpenUrl {
                url: "https://example.com".into()
            }
            .validate()
            .is_ok()
        );
        let action = TypedAction::CopyDerivedText {
            text: "secret output".into(),
        };
        assert!(!format!("{action:?}").contains("secret output"));
        assert!(RecognizerInput::new(ContentKind::Text, Some("secret"), true).is_err());

        let candidate = ActionCandidate {
            label: "private label".into(),
            confidence: 80,
            action,
        };
        assert!(!format!("{candidate:?}").contains("private label"));
    }
}
