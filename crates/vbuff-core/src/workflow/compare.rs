//! Text comparison and immutable transform previews.

use std::collections::VecDeque;
use std::fmt;

use regex::Regex;
use similar::{ChangeTag, TextDiff};
use thiserror::Error;

const MAX_TEXT_BYTES: usize = 1024 * 1024;
const MAX_DIFF_UNITS: usize = 16_384;
const MAX_DIFF_CHUNKS: usize = 32_768;
const MAX_REGEX_BYTES: usize = 4 * 1024;
const MAX_REPLACEMENT_BYTES: usize = 64 * 1024;
const MAX_TRANSFORM_HISTORY: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffMode {
    Lines,
    Words,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffKind {
    Equal,
    Removed,
    Added,
}

#[derive(Clone, PartialEq, Eq)]
pub struct DiffChunk {
    pub kind: DiffKind,
    pub text: String,
}

impl fmt::Debug for DiffChunk {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiffChunk")
            .field("kind", &self.kind)
            .field(
                "text",
                &format_args!("[redacted; {} bytes]", self.text.len()),
            )
            .finish()
    }
}

#[derive(Clone, Error, Debug, PartialEq, Eq)]
pub enum CompareError {
    #[error("text exceeds the workflow byte limit")]
    TooLarge,
    #[error("text has too many comparison units")]
    TooManyUnits,
    #[error("diff has too many output chunks")]
    TooManyChunks,
    #[error("transform regular expression is invalid")]
    InvalidRegex,
    #[error("transform input is not valid JSON")]
    InvalidJson,
    #[error("transform history does not match the canonical input")]
    CanonicalMismatch,
    #[error("transform history entry was not found")]
    HistoryNotFound,
}

pub fn compare_text(
    left: &str,
    right: &str,
    mode: DiffMode,
) -> Result<Vec<DiffChunk>, CompareError> {
    validate_text(left)?;
    validate_text(right)?;
    let units = match mode {
        DiffMode::Lines => left.lines().count().max(right.lines().count()),
        DiffMode::Words => left
            .split_whitespace()
            .count()
            .max(right.split_whitespace().count()),
    };
    if units > MAX_DIFF_UNITS {
        return Err(CompareError::TooManyUnits);
    }

    let diff = match mode {
        DiffMode::Lines => TextDiff::from_lines(left, right),
        DiffMode::Words => TextDiff::from_words(left, right),
    };
    let mut chunks: Vec<DiffChunk> = Vec::new();
    for change in diff.iter_all_changes() {
        let kind = match change.tag() {
            ChangeTag::Equal => DiffKind::Equal,
            ChangeTag::Delete => DiffKind::Removed,
            ChangeTag::Insert => DiffKind::Added,
        };
        let value = change.value();
        if let Some(last) = chunks.last_mut().filter(|last| last.kind == kind) {
            if last.text.len().saturating_add(value.len()) > MAX_TEXT_BYTES * 2 {
                return Err(CompareError::TooLarge);
            }
            last.text.push_str(value);
        } else {
            if chunks.len() >= MAX_DIFF_CHUNKS {
                return Err(CompareError::TooManyChunks);
            }
            chunks.push(DiffChunk {
                kind,
                text: value.to_owned(),
            });
        }
    }
    Ok(chunks)
}

#[derive(Clone, PartialEq, Eq)]
pub enum TextTransform {
    Trim,
    Uppercase,
    Lowercase,
    PrettyJson,
    RegexReplace {
        pattern: String,
        replacement: String,
    },
}

impl fmt::Debug for TextTransform {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Trim => formatter.write_str("Trim"),
            Self::Uppercase => formatter.write_str("Uppercase"),
            Self::Lowercase => formatter.write_str("Lowercase"),
            Self::PrettyJson => formatter.write_str("PrettyJson"),
            Self::RegexReplace {
                pattern,
                replacement,
            } => formatter
                .debug_struct("RegexReplace")
                .field(
                    "pattern",
                    &format_args!("[redacted; {} bytes]", pattern.len()),
                )
                .field(
                    "replacement",
                    &format_args!("[redacted; {} bytes]", replacement.len()),
                )
                .finish(),
        }
    }
}

impl TextTransform {
    pub fn apply(&self, input: &str) -> Result<String, CompareError> {
        validate_text(input)?;
        let output = match self {
            Self::Trim => input.trim().to_owned(),
            Self::Uppercase => input.to_uppercase(),
            Self::Lowercase => input.to_lowercase(),
            Self::PrettyJson => {
                let value: serde_json::Value =
                    serde_json::from_str(input).map_err(|_| CompareError::InvalidJson)?;
                serde_json::to_string_pretty(&value).map_err(|_| CompareError::InvalidJson)?
            }
            Self::RegexReplace {
                pattern,
                replacement,
            } => {
                if pattern.is_empty()
                    || pattern.len() > MAX_REGEX_BYTES
                    || replacement.len() > MAX_REPLACEMENT_BYTES
                {
                    return Err(CompareError::InvalidRegex);
                }
                Regex::new(pattern)
                    .map_err(|_| CompareError::InvalidRegex)?
                    .replace_all(input, replacement.as_str())
                    .into_owned()
            }
        };
        validate_text(&output)?;
        Ok(output)
    }

    pub const fn label(&self) -> &'static str {
        match self {
            Self::Trim => "Trim whitespace",
            Self::Uppercase => "Uppercase",
            Self::Lowercase => "Lowercase",
            Self::PrettyJson => "Format JSON",
            Self::RegexReplace { .. } => "Regex replace",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct TransformOverlay {
    canonical_hash: [u8; 32],
    transform: TextTransform,
    output: String,
    output_hash: [u8; 32],
}

impl fmt::Debug for TransformOverlay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransformOverlay")
            .field("canonical_hash", &"[redacted]")
            .field("transform", &self.transform)
            .field(
                "output",
                &format_args!("[redacted; {} bytes]", self.output.len()),
            )
            .field("output_hash", &"[redacted]")
            .finish()
    }
}

impl TransformOverlay {
    pub fn preview(canonical: &str, transform: TextTransform) -> Result<Self, CompareError> {
        validate_text(canonical)?;
        let output = transform.apply(canonical)?;
        Ok(Self {
            canonical_hash: digest(canonical),
            transform,
            output_hash: digest(&output),
            output,
        })
    }

    pub fn canonical_matches(&self, canonical: &str) -> bool {
        self.canonical_hash == digest(canonical)
    }

    pub fn transform(&self) -> &TextTransform {
        &self.transform
    }

    pub fn output(&self) -> &str {
        &self.output
    }

    pub fn output_hash(&self) -> [u8; 32] {
        self.output_hash
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct TransformRecord {
    pub sequence: u64,
    pub input_hash: [u8; 32],
    pub transform: TextTransform,
    output: String,
    pub output_hash: [u8; 32],
}

impl TransformRecord {
    pub fn output(&self) -> &str {
        &self.output
    }
}

impl fmt::Debug for TransformRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransformRecord")
            .field("sequence", &self.sequence)
            .field("input_hash", &"[redacted]")
            .field("transform", &self.transform)
            .field(
                "output",
                &format_args!("[redacted; {} bytes]", self.output.len()),
            )
            .field("output_hash", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct TransformHistory {
    canonical_hash: Option<[u8; 32]>,
    records: VecDeque<TransformRecord>,
    next_sequence: u64,
}

impl fmt::Debug for TransformHistory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransformHistory")
            .field("canonical_hash", &self.canonical_hash.map(|_| "[redacted]"))
            .field("records", &self.records)
            .field("next_sequence", &self.next_sequence)
            .finish()
    }
}

impl TransformHistory {
    pub fn records(&self) -> &VecDeque<TransformRecord> {
        &self.records
    }

    pub fn apply(
        &mut self,
        canonical: &str,
        transform: TextTransform,
    ) -> Result<&TransformRecord, CompareError> {
        let canonical_hash = digest(canonical);
        if self
            .canonical_hash
            .is_some_and(|existing| existing != canonical_hash)
        {
            return Err(CompareError::CanonicalMismatch);
        }
        self.canonical_hash = Some(canonical_hash);
        let input = self
            .records
            .back()
            .map_or(canonical, TransformRecord::output);
        let input_hash = digest(input);
        let output = transform.apply(input)?;
        self.next_sequence = self.next_sequence.wrapping_add(1).max(1);
        if self.records.len() >= MAX_TRANSFORM_HISTORY {
            self.records.pop_front();
        }
        self.records.push_back(TransformRecord {
            sequence: self.next_sequence,
            input_hash,
            transform,
            output_hash: digest(&output),
            output,
        });
        Ok(self.records.back().expect("record was just appended"))
    }

    pub fn replay(&self, canonical: &str, sequence: u64) -> Result<&str, CompareError> {
        if self.canonical_hash != Some(digest(canonical)) {
            return Err(CompareError::CanonicalMismatch);
        }
        self.records
            .iter()
            .find(|record| record.sequence == sequence)
            .map(TransformRecord::output)
            .ok_or(CompareError::HistoryNotFound)
    }

    pub fn reset(&mut self) {
        self.canonical_hash = None;
        self.records.clear();
    }
}

fn validate_text(text: &str) -> Result<(), CompareError> {
    if text.len() > MAX_TEXT_BYTES {
        Err(CompareError::TooLarge)
    } else {
        Ok(())
    }
}

fn digest(text: &str) -> [u8; 32] {
    *blake3::hash(text.as_bytes()).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_lines_and_words_without_exposing_text_in_debug() {
        let line = compare_text("one\ntwo\n", "one\nthree\n", DiffMode::Lines).unwrap();
        assert!(line.iter().any(|chunk| chunk.kind == DiffKind::Removed));
        assert!(line.iter().any(|chunk| chunk.kind == DiffKind::Added));
        let words = compare_text("copy old value", "copy new value", DiffMode::Words).unwrap();
        assert_eq!(
            words
                .iter()
                .filter(|chunk| chunk.kind != DiffKind::Equal)
                .count(),
            2
        );
        assert!(!format!("{words:?}").contains("new"));
    }

    #[test]
    fn overlay_never_mutates_canonical_and_history_replays_bounded_results() {
        let canonical = "  private value  ";
        let overlay = TransformOverlay::preview(canonical, TextTransform::Trim).unwrap();
        assert_eq!(canonical, "  private value  ");
        assert_eq!(overlay.output(), "private value");
        assert!(overlay.canonical_matches(canonical));
        assert!(!format!("{overlay:?}").contains("private"));

        let mut history = TransformHistory::default();
        let first = history
            .apply(canonical, TextTransform::Trim)
            .unwrap()
            .sequence;
        history.apply(canonical, TextTransform::Uppercase).unwrap();
        assert_eq!(history.replay(canonical, first).unwrap(), "private value");
        assert_eq!(history.records().back().unwrap().output(), "PRIVATE VALUE");
        assert_eq!(
            history.replay("different", first),
            Err(CompareError::CanonicalMismatch)
        );
        assert!(!format!("{history:?}").contains("PRIVATE VALUE"));
    }

    #[test]
    fn transform_inputs_and_outputs_are_bounded() {
        let invalid = TextTransform::RegexReplace {
            pattern: "(".into(),
            replacement: "x".into(),
        };
        assert_eq!(invalid.apply("text"), Err(CompareError::InvalidRegex));
        assert_eq!(
            TextTransform::PrettyJson.apply("not json"),
            Err(CompareError::InvalidJson)
        );
        assert_eq!(
            compare_text(&"x".repeat(MAX_TEXT_BYTES + 1), "", DiffMode::Lines),
            Err(CompareError::TooLarge)
        );
    }
}
