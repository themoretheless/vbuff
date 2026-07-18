use std::fmt;

use serde::{Deserialize, Serialize};

use super::IntegrationContractError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorTargetKind {
    Code,
    Markdown,
    Comment,
    Terminal,
    PlainText,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorCaptureMetadata {
    pub language: Option<String>,
    pub file_path: Option<String>,
    pub repository: Option<String>,
}

impl fmt::Debug for EditorCaptureMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EditorCaptureMetadata")
            .field("language", &self.language)
            .field(
                "file_path",
                &self.file_path.as_ref().map(|value| value.len()),
            )
            .field(
                "repository",
                &self.repository.as_ref().map(|value| value.len()),
            )
            .finish()
    }
}

impl EditorCaptureMetadata {
    pub fn validate(&self) -> Result<(), IntegrationContractError> {
        for value in [&self.language, &self.file_path, &self.repository]
            .into_iter()
            .flatten()
        {
            if value.is_empty() || value.len() > 4_096 || value.chars().any(char::is_control) {
                return Err(IntegrationContractError::InvalidField);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorPasteContext {
    pub target: EditorTargetKind,
    pub language: Option<String>,
    pub indentation_width: u8,
    pub use_tabs: bool,
}

impl EditorPasteContext {
    pub fn validate(&self) -> Result<(), IntegrationContractError> {
        if !(1..=16).contains(&self.indentation_width)
            || self.language.as_ref().is_some_and(|language| {
                language.is_empty() || language.len() > 64 || language.chars().any(char::is_control)
            })
        {
            return Err(IntegrationContractError::InvalidField);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_context_is_bounded_before_it_reaches_capture_or_paste() {
        assert!(
            EditorCaptureMetadata {
                language: Some("rust".into()),
                file_path: Some("src/main.rs".into()),
                repository: Some("vbuff".into()),
            }
            .validate()
            .is_ok()
        );
        let metadata = EditorCaptureMetadata {
            language: Some("rust".into()),
            file_path: Some("/Users/alice/private/main.rs".into()),
            repository: Some("secret-project".into()),
        };
        let debug = format!("{metadata:?}");
        assert!(!debug.contains("alice"));
        assert!(!debug.contains("secret-project"));
        assert!(
            EditorPasteContext {
                target: EditorTargetKind::Code,
                language: Some("rust".into()),
                indentation_width: 0,
                use_tabs: false,
            }
            .validate()
            .is_err()
        );
    }
}
