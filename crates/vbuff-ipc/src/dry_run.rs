use serde::{Deserialize, Serialize};
use vbuff_types::ClipId;

const MAX_PREVIEW_BYTES: usize = 1024 * 1024;
const MAX_WARNINGS: usize = 32;
const MAX_WARNING_BYTES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DryRunRequest {
    pub request_id: String,
    pub pipeline_id: String,
    pub clip_id: ClipId,
    pub preview_bytes: usize,
}

impl DryRunRequest {
    pub fn validate(&self, maximum_preview_bytes: usize) -> Result<(), &'static str> {
        if !valid_id(&self.request_id) {
            return Err("request_id_invalid");
        }
        if !valid_id(&self.pipeline_id) {
            return Err("pipeline_id_invalid");
        }
        if self.preview_bytes > maximum_preview_bytes.min(MAX_PREVIEW_BYTES) {
            return Err("preview_too_large");
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DryRunPreview {
    pub request_id: String,
    pub output_mime: String,
    pub output_bytes: u64,
    pub output_hash: [u8; 32],
    pub changed: bool,
    pub bounded_preview: String,
    pub warnings: Vec<String>,
}

impl DryRunPreview {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_id(&self.request_id) {
            return Err("request_id_invalid");
        }
        if self.output_mime.is_empty()
            || self.output_mime.len() > 256
            || self.output_mime.chars().any(char::is_control)
        {
            return Err("output_mime_invalid");
        }
        if self.bounded_preview.len() > MAX_PREVIEW_BYTES {
            return Err("preview_too_large");
        }
        if self.warnings.len() > MAX_WARNINGS
            || self.warnings.iter().any(|warning| {
                warning.is_empty()
                    || warning.len() > MAX_WARNING_BYTES
                    || warning.chars().any(char::is_control)
            })
        {
            return Err("warnings_invalid");
        }
        Ok(())
    }
}

impl std::fmt::Debug for DryRunPreview {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DryRunPreview")
            .field("request_id", &self.request_id)
            .field("output_mime", &self.output_mime)
            .field("output_bytes", &self.output_bytes)
            .field("output_hash", &"[redacted]")
            .field("changed", &self.changed)
            .field("bounded_preview", &"[redacted]")
            .field("warning_count", &self.warnings.len())
            .finish()
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_debug_redacts_preview() {
        let preview = DryRunPreview {
            request_id: "1".into(),
            output_mime: "text/plain".into(),
            output_bytes: 6,
            output_hash: [0; 32],
            changed: true,
            bounded_preview: "secret".into(),
            warnings: Vec::new(),
        };
        let debug = format!("{preview:?}");
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("0, 0, 0"));
    }

    #[test]
    fn dry_run_request_bounds_ids_and_preview() {
        let mut request = DryRunRequest {
            request_id: "request-1".into(),
            pipeline_id: "clean.url".into(),
            clip_id: ClipId::new(),
            preview_bytes: 512,
        };
        assert!(request.validate(512).is_ok());
        request.preview_bytes = 513;
        assert_eq!(request.validate(512), Err("preview_too_large"));
        request.preview_bytes = MAX_PREVIEW_BYTES + 1;
        assert_eq!(request.validate(usize::MAX), Err("preview_too_large"));
    }
}
