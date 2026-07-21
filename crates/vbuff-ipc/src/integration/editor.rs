use std::fmt;

use serde::{Deserialize, Serialize};

use super::IntegrationContractError;

const MAX_EDITOR_INPUT_BYTES: usize = 1_024 * 1_024;
const MAX_EDITOR_OUTPUT_BYTES: usize = 2 * 1_024 * 1_024;

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
    pub branch: Option<String>,
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
            .field("branch", &self.branch.as_ref().map(|value| value.len()))
            .finish()
    }
}

impl EditorCaptureMetadata {
    pub fn validate(&self) -> Result<(), IntegrationContractError> {
        if self
            .language
            .as_ref()
            .is_some_and(|language| !valid_language_id(language))
            || self
                .file_path
                .as_ref()
                .is_some_and(|path| !valid_text_field(path, 4_096))
            || self
                .repository
                .as_ref()
                .is_some_and(|repository| !valid_text_field(repository, 256))
            || self
                .branch
                .as_ref()
                .is_some_and(|branch| !valid_text_field(branch, 256))
        {
            return Err(IntegrationContractError::InvalidField);
        }
        Ok(())
    }
}

pub fn adapt_text_for_editor(
    input: &str,
    context: &EditorPasteContext,
    base_indent_levels: u8,
) -> Result<String, IntegrationContractError> {
    if input.len() > MAX_EDITOR_INPUT_BYTES || input.contains('\0') || base_indent_levels > 64 {
        return Err(IntegrationContractError::InvalidField);
    }
    context.validate()?;
    let newline = if input.contains("\r\n") { "\r\n" } else { "\n" };
    let mut body = input;
    if context.target == EditorTargetKind::Code
        && let Some(stripped) = strip_markdown_fence(body, newline)
    {
        body = stripped;
    }
    if context.target == EditorTargetKind::Markdown && !is_markdown_fenced(body) {
        let language = context.language.as_deref().unwrap_or_default();
        let fence = markdown_fence(body);
        let output_bytes = body
            .len()
            .checked_add(language.len())
            .and_then(|bytes| bytes.checked_add(fence.len().saturating_mul(2)))
            .and_then(|bytes| bytes.checked_add(newline.len().saturating_mul(2)))
            .ok_or(IntegrationContractError::InvalidField)?;
        if output_bytes > MAX_EDITOR_OUTPUT_BYTES {
            return Err(IntegrationContractError::InvalidField);
        }
        let mut output = String::with_capacity(output_bytes);
        output.push_str(&fence);
        output.push_str(language);
        output.push_str(newline);
        output.push_str(body);
        output.push_str(newline);
        output.push_str(&fence);
        return Ok(output);
    }
    if !matches!(
        context.target,
        EditorTargetKind::Code | EditorTargetKind::Comment
    ) {
        return Ok(body.to_owned());
    }

    let common = body
        .split(newline)
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.chars()
                .take_while(|character| matches!(character, ' ' | '\t'))
                .count()
        })
        .min()
        .unwrap_or(0);
    let prefix = if context.use_tabs {
        "\t".repeat(usize::from(base_indent_levels))
    } else {
        " ".repeat(usize::from(base_indent_levels) * usize::from(context.indentation_width))
    };
    let mut adjusted = String::with_capacity(body.len().min(MAX_EDITOR_OUTPUT_BYTES));
    for (index, line) in body.split(newline).enumerate() {
        let content = if line.trim().is_empty() {
            ""
        } else {
            let byte = line
                .char_indices()
                .nth(common)
                .map_or(line.len(), |(index, _)| index);
            &line[byte..]
        };
        let added_bytes = usize::from(index > 0)
            .saturating_mul(newline.len())
            .saturating_add(if content.is_empty() { 0 } else { prefix.len() })
            .saturating_add(content.len());
        if adjusted.len().saturating_add(added_bytes) > MAX_EDITOR_OUTPUT_BYTES {
            return Err(IntegrationContractError::InvalidField);
        }
        if index > 0 {
            adjusted.push_str(newline);
        }
        if !content.is_empty() {
            adjusted.push_str(&prefix);
            adjusted.push_str(content);
        }
    }
    Ok(adjusted)
}

fn is_markdown_fenced(input: &str) -> bool {
    input.trim_start().starts_with("```") && input.trim_end().ends_with("```")
}

fn strip_markdown_fence<'a>(input: &'a str, newline: &str) -> Option<&'a str> {
    let first_end = input.find(newline)?;
    input[..first_end]
        .trim_start()
        .starts_with("```")
        .then_some(())?;
    let closing = format!("{newline}```");
    let body = input.get(first_end + newline.len()..)?;
    body.strip_suffix(&closing)
}

fn markdown_fence(input: &str) -> String {
    let (longest_run, _) = input
        .bytes()
        .fold((0_usize, 0_usize), |(longest, current), byte| {
            if byte == b'`' {
                let current = current.saturating_add(1);
                (longest.max(current), current)
            } else {
                (longest, 0)
            }
        });
    "`".repeat(longest_run.saturating_add(1).max(3))
}

fn valid_language_id(language: &str) -> bool {
    !language.is_empty()
        && language.len() <= 64
        && language.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'#' | b'-' | b'_' | b'.')
        })
}

fn valid_text_field(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= maximum_bytes && !value.chars().any(char::is_control)
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
            || self
                .language
                .as_ref()
                .is_some_and(|language| !valid_language_id(language))
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
                branch: Some("main".into()),
            }
            .validate()
            .is_ok()
        );
        let metadata = EditorCaptureMetadata {
            language: Some("rust".into()),
            file_path: Some("/Users/alice/private/main.rs".into()),
            repository: Some("secret-project".into()),
            branch: Some("private-branch".into()),
        };
        let debug = format!("{metadata:?}");
        assert!(!debug.contains("alice"));
        assert!(!debug.contains("secret-project"));
        assert!(!debug.contains("private-branch"));
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
        assert!(
            EditorPasteContext {
                target: EditorTargetKind::Markdown,
                language: Some("rust```unsafe".into()),
                indentation_width: 4,
                use_tabs: false,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn editor_adaptation_preserves_line_endings_and_only_changes_requested_shape() {
        let code = EditorPasteContext {
            target: EditorTargetKind::Code,
            language: Some("rust".into()),
            indentation_width: 4,
            use_tabs: false,
        };
        assert_eq!(
            adapt_text_for_editor("```rust\r\n    one\r\n      two\r\n```", &code, 1).unwrap(),
            "    one\r\n      two"
        );
        let markdown = EditorPasteContext {
            target: EditorTargetKind::Markdown,
            ..code.clone()
        };
        assert_eq!(
            adapt_text_for_editor("fn main() {}", &markdown, 0).unwrap(),
            "```rust\nfn main() {}\n```"
        );
        let terminal = EditorPasteContext {
            target: EditorTargetKind::Terminal,
            ..code
        };
        assert_eq!(
            adapt_text_for_editor("  printf 'x'", &terminal, 4).unwrap(),
            "  printf 'x'"
        );

        let expansion = "x\n".repeat(3_000);
        let wide_indent = EditorPasteContext {
            target: EditorTargetKind::Code,
            language: Some("rust".into()),
            indentation_width: 16,
            use_tabs: false,
        };
        assert!(adapt_text_for_editor(&expansion, &wide_indent, 64).is_err());

        let fence_collision = "before ``` after";
        let fenced = adapt_text_for_editor(fence_collision, &markdown, 0).unwrap();
        assert!(fenced.starts_with("````rust\n"));
        assert!(fenced.ends_with("\n````"));
    }
}
