use serde::{Deserialize, Serialize};
use vbuff_types::{ContentKind, Flavor};

use crate::Result;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportRecord {
    pub flavors: Vec<Flavor>,
    pub kind_hint: Option<ContentKind>,
    pub created_at_ms: Option<i64>,
    pub source_label: Option<String>,
}

impl std::fmt::Debug for ImportRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImportRecord")
            .field("flavor_count", &self.flavors.len())
            .field("kind_hint", &self.kind_hint)
            .field("created_at_ms", &self.created_at_ms)
            .field(
                "source_label",
                &self.source_label.as_ref().map(|_| "[redacted]"),
            )
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportRecord {
    pub flavors: Vec<Flavor>,
    pub kind: ContentKind,
    pub created_at_ms: i64,
}

impl std::fmt::Debug for ExportRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExportRecord")
            .field("flavor_count", &self.flavors.len())
            .field("kind", &self.kind)
            .field("created_at_ms", &self.created_at_ms)
            .finish()
    }
}

pub trait ImportAdapter: Send + Sync {
    fn format_id(&self) -> &'static str;
    fn confidence(&self, prefix: &[u8]) -> u8;
    fn import(&self, bytes: &[u8]) -> Result<Vec<ImportRecord>>;
}

pub trait ExportAdapter: Send + Sync {
    fn format_id(&self) -> &'static str;
    fn export(&self, records: &[ExportRecord]) -> Result<Vec<u8>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_record_debug_is_content_free() {
        let record = ImportRecord {
            flavors: vec![Flavor::inline("text/plain", b"private value".to_vec())],
            kind_hint: Some(ContentKind::Text),
            created_at_ms: Some(1),
            source_label: Some("private.app".into()),
        };
        let debug = format!("{record:?}");
        assert!(!debug.contains("private value"));
        assert!(!debug.contains("private.app"));
    }
}
