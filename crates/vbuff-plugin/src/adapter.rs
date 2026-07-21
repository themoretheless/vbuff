use serde::{Deserialize, Serialize};
use vbuff_types::{Body, ContentKind, Flavor, FlavorRealization};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdapterLimits {
    pub maximum_input_bytes: usize,
    pub maximum_records: usize,
    pub maximum_flavors_per_record: usize,
    pub maximum_total_payload_bytes: u64,
    pub maximum_output_bytes: usize,
}

impl Default for AdapterLimits {
    fn default() -> Self {
        Self {
            maximum_input_bytes: 64 * 1024 * 1024,
            maximum_records: 100_000,
            maximum_flavors_per_record: 32,
            maximum_total_payload_bytes: 512 * 1024 * 1024,
            maximum_output_bytes: 64 * 1024 * 1024,
        }
    }
}

impl AdapterLimits {
    fn validate(self) -> Result<()> {
        if self.maximum_input_bytes == 0
            || self.maximum_input_bytes > 512 * 1024 * 1024
            || self.maximum_records == 0
            || self.maximum_records > 1_000_000
            || self.maximum_flavors_per_record == 0
            || self.maximum_flavors_per_record > 128
            || self.maximum_total_payload_bytes == 0
            || self.maximum_total_payload_bytes > 4 * 1024 * 1024 * 1024
            || self.maximum_output_bytes == 0
            || self.maximum_output_bytes > 512 * 1024 * 1024
        {
            return Err(crate::PluginError::InvalidInput(
                "adapter limits are invalid".into(),
            ));
        }
        Ok(())
    }
}

pub fn run_import_adapter(
    adapter: &dyn ImportAdapter,
    bytes: &[u8],
    limits: AdapterLimits,
) -> Result<Vec<ImportRecord>> {
    limits.validate()?;
    validate_format_id(adapter.format_id())?;
    if bytes.len() > limits.maximum_input_bytes {
        return Err(crate::PluginError::InvalidInput(
            "adapter input exceeds the byte limit".into(),
        ));
    }
    let records = adapter.import(bytes)?;
    validate_import_records(&records, limits)?;
    Ok(records)
}

pub fn run_export_adapter(
    adapter: &dyn ExportAdapter,
    records: &[ExportRecord],
    limits: AdapterLimits,
) -> Result<Vec<u8>> {
    limits.validate()?;
    validate_format_id(adapter.format_id())?;
    if records.len() > limits.maximum_records {
        return Err(crate::PluginError::InvalidInput(
            "adapter record count exceeds the limit".into(),
        ));
    }
    let mut total = 0_u64;
    for record in records {
        validate_flavors(&record.flavors, limits, &mut total)?;
    }
    let output = adapter.export(records)?;
    if output.len() > limits.maximum_output_bytes {
        return Err(crate::PluginError::InvalidInput(
            "adapter output exceeds the byte limit".into(),
        ));
    }
    Ok(output)
}

fn validate_import_records(records: &[ImportRecord], limits: AdapterLimits) -> Result<()> {
    if records.len() > limits.maximum_records {
        return Err(crate::PluginError::InvalidInput(
            "adapter record count exceeds the limit".into(),
        ));
    }
    let mut total = 0_u64;
    for record in records {
        if record.source_label.as_ref().is_some_and(|label| {
            label.is_empty() || label.len() > 256 || label.chars().any(char::is_control)
        }) {
            return Err(crate::PluginError::InvalidInput(
                "adapter source label is invalid".into(),
            ));
        }
        validate_flavors(&record.flavors, limits, &mut total)?;
    }
    Ok(())
}

fn validate_flavors(flavors: &[Flavor], limits: AdapterLimits, total: &mut u64) -> Result<()> {
    if flavors.is_empty() || flavors.len() > limits.maximum_flavors_per_record {
        return Err(crate::PluginError::InvalidInput(
            "adapter flavor count is invalid".into(),
        ));
    }
    for flavor in flavors {
        if flavor.mime.is_empty()
            || flavor.mime.len() > 256
            || flavor.mime.chars().any(char::is_control)
        {
            return Err(crate::PluginError::InvalidInput(
                "adapter MIME type is invalid".into(),
            ));
        }
        let Body::Inline(bytes) = &flavor.body else {
            return Err(crate::PluginError::InvalidInput(
                "adapter flavors must contain hydrated inline bytes".into(),
            ));
        };
        if flavor.realization != FlavorRealization::Realized
            || flavor
                .integrity_hash
                .is_some_and(|expected| expected != *blake3::hash(bytes).as_bytes())
        {
            return Err(crate::PluginError::InvalidInput(
                "adapter flavor realization or integrity is invalid".into(),
            ));
        }
        *total = total
            .checked_add(u64::try_from(bytes.len()).map_err(|_| {
                crate::PluginError::InvalidInput("adapter payload size is invalid".into())
            })?)
            .ok_or_else(|| {
                crate::PluginError::InvalidInput("adapter payload total overflowed".into())
            })?;
        if *total > limits.maximum_total_payload_bytes {
            return Err(crate::PluginError::InvalidInput(
                "adapter payload total exceeds the limit".into(),
            ));
        }
    }
    Ok(())
}

fn validate_format_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        })
    {
        return Err(crate::PluginError::InvalidInput(
            "adapter format id is invalid".into(),
        ));
    }
    Ok(())
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

    struct TestImport;

    impl ImportAdapter for TestImport {
        fn format_id(&self) -> &'static str {
            "test.v1"
        }

        fn confidence(&self, _prefix: &[u8]) -> u8 {
            100
        }

        fn import(&self, bytes: &[u8]) -> Result<Vec<ImportRecord>> {
            Ok(vec![ImportRecord {
                flavors: vec![Flavor::inline("text/plain", bytes.to_vec())],
                kind_hint: Some(ContentKind::Text),
                created_at_ms: None,
                source_label: Some("test".into()),
            }])
        }
    }

    struct OversizeExport;

    impl ExportAdapter for OversizeExport {
        fn format_id(&self) -> &'static str {
            "test.v1"
        }

        fn export(&self, _records: &[ExportRecord]) -> Result<Vec<u8>> {
            Ok(vec![0; 5])
        }
    }

    struct SpilledImport;

    impl ImportAdapter for SpilledImport {
        fn format_id(&self) -> &'static str {
            "test.v1"
        }

        fn confidence(&self, _prefix: &[u8]) -> u8 {
            100
        }

        fn import(&self, _bytes: &[u8]) -> Result<Vec<ImportRecord>> {
            Ok(vec![ImportRecord {
                flavors: vec![Flavor {
                    mime: "text/plain".into(),
                    body: Body::Spilled {
                        blob_ref: "internal-cas-reference".into(),
                        byte_size: 4,
                    },
                    origin: Default::default(),
                    realization: FlavorRealization::Realized,
                    integrity_hash: None,
                }],
                kind_hint: Some(ContentKind::Text),
                created_at_ms: None,
                source_label: Some("test".into()),
            }])
        }
    }

    #[test]
    fn adapter_sdk_enforces_host_limits_before_accepting_results() {
        let limits = AdapterLimits {
            maximum_input_bytes: 4,
            maximum_records: 1,
            maximum_flavors_per_record: 1,
            maximum_total_payload_bytes: 4,
            maximum_output_bytes: 4,
        };
        assert_eq!(
            run_import_adapter(&TestImport, b"clip", limits).unwrap()[0].flavors[0].as_text(),
            Some("clip")
        );
        assert!(run_import_adapter(&TestImport, b"large", limits).is_err());
        assert!(run_import_adapter(&SpilledImport, b"clip", limits).is_err());
        assert!(run_export_adapter(&OversizeExport, &[], limits).is_err());
    }
}
