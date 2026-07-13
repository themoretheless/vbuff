use serde::{Deserialize, Serialize};

use crate::{PluginError, Result};

const MAX_PIPELINE_STEPS: usize = 64;
const MAX_VALUE_BYTES: usize = 16 * 1024 * 1024;
const MAX_PREFIX_BYTES: usize = 64 * 1024;
const MAX_PREVIEW_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    Text,
    Url,
    Json,
    Bytes,
}

#[derive(Clone, PartialEq, Eq)]
pub struct TypedValue {
    pub kind: ValueType,
    pub mime: String,
    pub bytes: Vec<u8>,
}

impl std::fmt::Debug for TypedValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TypedValue")
            .field("kind", &self.kind)
            .field("mime", &self.mime)
            .field(
                "bytes",
                &format_args!("[redacted; {} bytes]", self.bytes.len()),
            )
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "transform", rename_all = "snake_case")]
pub enum TransformSpec {
    TrimText,
    UppercaseText,
    PrefixText { prefix: String },
    NormalizeUrl,
    PrettyJson,
}

impl std::fmt::Debug for TransformSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TrimText => formatter.write_str("TrimText"),
            Self::UppercaseText => formatter.write_str("UppercaseText"),
            Self::PrefixText { prefix } => formatter
                .debug_struct("PrefixText")
                .field(
                    "prefix",
                    &format_args!("[redacted; {} bytes]", prefix.len()),
                )
                .finish(),
            Self::NormalizeUrl => formatter.write_str("NormalizeUrl"),
            Self::PrettyJson => formatter.write_str("PrettyJson"),
        }
    }
}

impl TransformSpec {
    pub const fn input_type(&self) -> ValueType {
        match self {
            Self::TrimText | Self::UppercaseText | Self::PrefixText { .. } => ValueType::Text,
            Self::NormalizeUrl => ValueType::Url,
            Self::PrettyJson => ValueType::Json,
        }
    }

    pub const fn output_type(&self) -> ValueType {
        self.input_type()
    }

    fn apply(&self, value: TypedValue) -> Result<TypedValue> {
        if value.kind != self.input_type() {
            return Err(PluginError::TypeMismatch(format!(
                "expected {:?}, received {:?}",
                self.input_type(),
                value.kind
            )));
        }
        let text = String::from_utf8(value.bytes)
            .map_err(|_| PluginError::InvalidInput("value is not UTF-8".into()))?;
        let output = match self {
            Self::TrimText => text.trim().to_owned(),
            Self::UppercaseText => text.to_uppercase(),
            Self::PrefixText { prefix } => format!("{prefix}{text}"),
            Self::NormalizeUrl => {
                let mut parsed = url::Url::parse(text.trim())
                    .map_err(|_| PluginError::InvalidInput("invalid URL".into()))?;
                parsed.set_fragment(None);
                parsed.to_string()
            }
            Self::PrettyJson => {
                let parsed: serde_json::Value = serde_json::from_str(&text)
                    .map_err(|_| PluginError::InvalidInput("invalid JSON".into()))?;
                serde_json::to_string_pretty(&parsed)
                    .map_err(|error| PluginError::Serialization(error.to_string()))?
            }
        };
        if output.len() > MAX_VALUE_BYTES {
            return Err(PluginError::InvalidInput(
                "transform output exceeds the byte limit".into(),
            ));
        }
        Ok(TypedValue {
            kind: self.output_type(),
            mime: value.mime,
            bytes: output.into_bytes(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pipeline {
    input_type: ValueType,
    steps: Vec<TransformSpec>,
}

impl Pipeline {
    pub fn new(input_type: ValueType, steps: Vec<TransformSpec>) -> Result<Self> {
        if steps.len() > MAX_PIPELINE_STEPS {
            return Err(PluginError::InvalidInput(
                "pipeline has too many steps".into(),
            ));
        }
        let mut current = input_type;
        for step in &steps {
            if matches!(step, TransformSpec::PrefixText { prefix } if prefix.len() > MAX_PREFIX_BYTES)
            {
                return Err(PluginError::InvalidInput(
                    "transform prefix exceeds the byte limit".into(),
                ));
            }
            if step.input_type() != current {
                return Err(PluginError::TypeMismatch(format!(
                    "pipeline has {:?} before a {:?} transform",
                    current,
                    step.input_type()
                )));
            }
            current = step.output_type();
        }
        Ok(Self { input_type, steps })
    }

    pub fn execute(&self, input: TypedValue) -> Result<TypedValue> {
        if input.kind != self.input_type {
            return Err(PluginError::TypeMismatch(format!(
                "pipeline expects {:?}, received {:?}",
                self.input_type, input.kind
            )));
        }
        if input.bytes.len() > MAX_VALUE_BYTES
            || input.mime.is_empty()
            || input.mime.len() > 256
            || input.mime.chars().any(char::is_control)
        {
            return Err(PluginError::InvalidInput(
                "input value exceeds its contract".into(),
            ));
        }
        self.steps
            .iter()
            .try_fold(input, |value, step| step.apply(value))
    }

    pub fn dry_run(&self, input: &TypedValue, preview_bytes: usize) -> Result<PipelinePreview> {
        if preview_bytes > MAX_PREVIEW_BYTES {
            return Err(PluginError::InvalidInput(
                "preview exceeds the byte limit".into(),
            ));
        }
        let output = self.execute(input.clone())?;
        let changed = output != *input;
        let output_hash = *blake3::hash(&output.bytes).as_bytes();
        let mut preview = String::from_utf8_lossy(&output.bytes).into_owned();
        truncate_utf8(&mut preview, preview_bytes);
        Ok(PipelinePreview {
            output_type: output.kind,
            output_mime: output.mime,
            output_bytes: output.bytes.len() as u64,
            output_hash,
            changed,
            bounded_preview: preview,
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct PipelinePreview {
    pub output_type: ValueType,
    pub output_mime: String,
    pub output_bytes: u64,
    pub output_hash: [u8; 32],
    pub changed: bool,
    pub bounded_preview: String,
}

impl std::fmt::Debug for PipelinePreview {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PipelinePreview")
            .field("output_type", &self.output_type)
            .field("output_mime", &self.output_mime)
            .field("output_bytes", &self.output_bytes)
            .field("output_hash", &"[redacted]")
            .field("changed", &self.changed)
            .field("bounded_preview", &"[redacted]")
            .finish()
    }
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
    fn typed_pipeline_rejects_invalid_composition() {
        assert!(Pipeline::new(ValueType::Text, vec![TransformSpec::NormalizeUrl]).is_err());
    }

    #[test]
    fn dry_run_is_deterministic_bounded_and_non_mutating() {
        let pipeline = Pipeline::new(
            ValueType::Text,
            vec![TransformSpec::TrimText, TransformSpec::UppercaseText],
        )
        .unwrap();
        let input = TypedValue {
            kind: ValueType::Text,
            mime: "text/plain".into(),
            bytes: b"  hello  ".to_vec(),
        };
        let first = pipeline.dry_run(&input, 3).unwrap();
        let second = pipeline.dry_run(&input, 3).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.bounded_preview, "HEL");
        assert_eq!(input.bytes, b"  hello  ");
        assert!(!format!("{first:?}").contains("HEL"));
        assert!(!format!("{first:?}").contains("0, 0, 0"));
    }

    #[test]
    fn pipeline_rejects_unbounded_steps_and_previews() {
        assert!(Pipeline::new(ValueType::Text, vec![TransformSpec::TrimText; 65]).is_err());
        let pipeline = Pipeline::new(ValueType::Text, Vec::new()).unwrap();
        let input = TypedValue {
            kind: ValueType::Text,
            mime: "text/plain".into(),
            bytes: b"hello".to_vec(),
        };
        assert!(pipeline.dry_run(&input, MAX_PREVIEW_BYTES + 1).is_err());
    }

    #[test]
    fn pipeline_debug_redacts_prefix_content() {
        let pipeline = Pipeline::new(
            ValueType::Text,
            vec![TransformSpec::PrefixText {
                prefix: "private prefix".into(),
            }],
        )
        .unwrap();
        assert!(!format!("{pipeline:?}").contains("private prefix"));
    }
}
