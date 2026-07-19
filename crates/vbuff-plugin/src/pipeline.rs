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

    pub const fn label(&self) -> &'static str {
        match self {
            Self::TrimText => "Trim whitespace",
            Self::UppercaseText => "Uppercase",
            Self::PrefixText { .. } => "Add prefix",
            Self::NormalizeUrl => "Normalize URL",
            Self::PrettyJson => "Format JSON",
        }
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
        self.validate_input(&input)?;
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

    pub fn dry_run_explained(
        &self,
        input: &TypedValue,
        preview_bytes: usize,
    ) -> Result<ExplainedPipelinePreview> {
        self.validate_input(input)?;
        if preview_bytes > MAX_PREVIEW_BYTES {
            return Err(PluginError::InvalidInput(
                "preview exceeds the byte limit".into(),
            ));
        }

        let mut current = input.clone();
        let mut steps = Vec::with_capacity(self.steps.len());
        for (index, transform) in self.steps.iter().enumerate() {
            let input_bytes = current.bytes.len() as u64;
            let input_hash = *blake3::hash(&current.bytes).as_bytes();
            let output = transform.apply(current)?;
            let output_hash = *blake3::hash(&output.bytes).as_bytes();
            steps.push(PipelineStepExplanation {
                index,
                label: transform.label().into(),
                input_type: transform.input_type(),
                output_type: transform.output_type(),
                input_bytes,
                output_bytes: output.bytes.len() as u64,
                changed: input_hash != output_hash,
            });
            current = output;
        }

        let changed = current != *input;
        let output_hash = *blake3::hash(&current.bytes).as_bytes();
        let mut bounded_preview = String::from_utf8_lossy(&current.bytes).into_owned();
        truncate_utf8(&mut bounded_preview, preview_bytes);
        Ok(ExplainedPipelinePreview {
            preview: PipelinePreview {
                output_type: current.kind,
                output_mime: current.mime,
                output_bytes: current.bytes.len() as u64,
                output_hash,
                changed,
                bounded_preview,
            },
            explanation: PipelineExplanation {
                input_type: self.input_type,
                output_type: current.kind,
                input_bytes: input.bytes.len() as u64,
                output_bytes: current.bytes.len() as u64,
                changed,
                steps,
            },
        })
    }

    pub fn graph(&self) -> PipelineGraph {
        PipelineGraph::from_steps(self.input_type, &self.steps)
    }

    pub const fn input_type(&self) -> ValueType {
        self.input_type
    }

    pub fn steps(&self) -> &[TransformSpec] {
        &self.steps
    }

    fn validate_input(&self, input: &TypedValue) -> Result<()> {
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
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineBuilder {
    input_type: ValueType,
    steps: Vec<TransformSpec>,
}

impl PipelineBuilder {
    pub const fn new(input_type: ValueType) -> Self {
        Self {
            input_type,
            steps: Vec::new(),
        }
    }

    pub fn from_pipeline(pipeline: &Pipeline) -> Self {
        Self {
            input_type: pipeline.input_type,
            steps: pipeline.steps.clone(),
        }
    }

    pub fn insert(&mut self, index: usize, transform: TransformSpec) -> Result<()> {
        if index > self.steps.len() || self.steps.len() >= MAX_PIPELINE_STEPS {
            return Err(PluginError::InvalidInput(
                "pipeline insertion is outside the step limit".into(),
            ));
        }
        let mut candidate = self.steps.clone();
        candidate.insert(index, transform);
        Pipeline::new(self.input_type, candidate.clone())?;
        self.steps = candidate;
        Ok(())
    }

    pub fn remove(&mut self, index: usize) -> Result<TransformSpec> {
        if index >= self.steps.len() {
            return Err(PluginError::InvalidInput(
                "pipeline step was not found".into(),
            ));
        }
        Ok(self.steps.remove(index))
    }

    pub fn move_step(&mut self, from: usize, to: usize) -> Result<()> {
        if from >= self.steps.len() || to >= self.steps.len() {
            return Err(PluginError::InvalidInput(
                "pipeline move is outside the step range".into(),
            ));
        }
        let mut candidate = self.steps.clone();
        let transform = candidate.remove(from);
        candidate.insert(to, transform);
        Pipeline::new(self.input_type, candidate.clone())?;
        self.steps = candidate;
        Ok(())
    }

    pub fn graph(&self) -> PipelineGraph {
        PipelineGraph::from_steps(self.input_type, &self.steps)
    }

    pub fn build(&self) -> Result<Pipeline> {
        Pipeline::new(self.input_type, self.steps.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineGraph {
    pub input_type: ValueType,
    pub output_type: ValueType,
    pub nodes: Vec<PipelineNode>,
    pub edges: Vec<PipelineEdge>,
}

impl PipelineGraph {
    fn from_steps(input_type: ValueType, steps: &[TransformSpec]) -> Self {
        let nodes = steps
            .iter()
            .enumerate()
            .map(|(index, transform)| PipelineNode {
                id: index as u16,
                label: transform.label().into(),
                input_type: transform.input_type(),
                output_type: transform.output_type(),
            })
            .collect::<Vec<_>>();
        let edges = (1..nodes.len())
            .map(|index| PipelineEdge {
                from: (index - 1) as u16,
                to: index as u16,
            })
            .collect();
        let output_type = steps.last().map_or(input_type, TransformSpec::output_type);
        Self {
            input_type,
            output_type,
            nodes,
            edges,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineNode {
    pub id: u16,
    pub label: String,
    pub input_type: ValueType,
    pub output_type: ValueType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineEdge {
    pub from: u16,
    pub to: u16,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineStepExplanation {
    pub index: usize,
    pub label: String,
    pub input_type: ValueType,
    pub output_type: ValueType,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineExplanation {
    pub input_type: ValueType,
    pub output_type: ValueType,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub changed: bool,
    pub steps: Vec<PipelineStepExplanation>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ExplainedPipelinePreview {
    pub preview: PipelinePreview,
    pub explanation: PipelineExplanation,
}

impl std::fmt::Debug for ExplainedPipelinePreview {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExplainedPipelinePreview")
            .field("preview", &self.preview)
            .field("explanation", &self.explanation)
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

    #[test]
    fn visual_builder_exposes_a_typed_linear_graph() {
        let mut builder = PipelineBuilder::new(ValueType::Text);
        builder.insert(0, TransformSpec::TrimText).unwrap();
        builder.insert(1, TransformSpec::UppercaseText).unwrap();
        builder.move_step(1, 0).unwrap();
        let graph = builder.graph();
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges, vec![PipelineEdge { from: 0, to: 1 }]);
        assert_eq!(builder.build().unwrap().steps().len(), 2);
    }

    #[test]
    fn explained_dry_run_reports_each_change_without_content() {
        let pipeline = Pipeline::new(
            ValueType::Text,
            vec![TransformSpec::TrimText, TransformSpec::UppercaseText],
        )
        .unwrap();
        let input = TypedValue {
            kind: ValueType::Text,
            mime: "text/plain".into(),
            bytes: b"  private value  ".to_vec(),
        };
        let explained = pipeline.dry_run_explained(&input, 7).unwrap();
        assert_eq!(explained.explanation.steps.len(), 2);
        assert!(explained.explanation.steps.iter().all(|step| step.changed));
        assert_eq!(explained.preview.bounded_preview, "PRIVATE");
        assert!(!format!("{explained:?}").contains("private value"));
    }
}
