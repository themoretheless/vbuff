//! Machine-readable startup and store diagnostics.

use serde::Serialize;
use vbuff_platform::lifecycle::SessionContext;
use vbuff_platform::{ProcessHardeningReport, SecurityPosture};
use vbuff_store::{Store, StoreDoctorReport, StoreOpenProfile};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DoctorFormat {
    Human,
    Json,
}

#[derive(Serialize)]
struct DoctorOutput {
    ok: bool,
    capture_allowed: bool,
    store_present: bool,
    version: &'static str,
    target_os: &'static str,
    session: SessionContext,
    process_hardening: ProcessHardeningReport,
    security_posture: SecurityPosture,
    store_open: StoreOpenProfile,
    store: StoreDoctorReport,
}

pub(crate) fn requested() -> Option<DoctorFormat> {
    let mut arguments = std::env::args().skip(1);
    if arguments.next().as_deref() != Some("doctor") {
        return None;
    }
    Some(if arguments.any(|argument| argument == "--json") {
        DoctorFormat::Json
    } else {
        DoctorFormat::Human
    })
}

pub(crate) fn run(
    format: DoctorFormat,
    process_hardening: ProcessHardeningReport,
    strict_mode: bool,
) -> anyhow::Result<()> {
    let path = vbuff_store::default_db_path()?;
    let store_present = path.exists();
    let (store, store_open) = if store_present {
        Store::open_read_only_profiled(&path)?
    } else {
        (Store::open_in_memory()?, StoreOpenProfile::default())
    };
    let store_report = store.doctor()?;
    let security_posture = SecurityPosture::detect(
        strict_mode,
        process_hardening.core_dumps_blocked,
        process_hardening.ptrace_blocked,
    );
    let output = DoctorOutput {
        ok: store_present && store_report.is_healthy() && security_posture.is_fully_protected(),
        capture_allowed: security_posture.strict_allows_capture(),
        store_present,
        version: env!("CARGO_PKG_VERSION"),
        target_os: std::env::consts::OS,
        session: SessionContext::detect(),
        process_hardening,
        security_posture,
        store_open,
        store: store_report,
    };
    match format {
        DoctorFormat::Json => println!("{}", serde_json::to_string_pretty(&output)?),
        DoctorFormat::Human => {
            println!(
                "vbuff doctor: {}",
                if output.ok {
                    "healthy"
                } else {
                    "attention needed"
                }
            );
            println!(
                "store present: {}; schema: {}/{}; rows: {}; FTS healthy: {}",
                output.store_present,
                output.store.schema_version,
                output.store.expected_schema_version,
                output.store.clip_rows,
                output.store.fts.is_healthy()
            );
            println!(
                "encryption: {}; strict capture allowed: {}",
                output
                    .store
                    .cipher_version
                    .as_deref()
                    .unwrap_or("not active"),
                output.security_posture.strict_allows_capture()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_output_schema_is_stable_and_content_free() {
        let store = Store::open_in_memory().unwrap();
        let store_report = store.doctor().unwrap();
        let output = DoctorOutput {
            ok: true,
            capture_allowed: true,
            store_present: true,
            version: "test",
            target_os: "test",
            session: SessionContext::detect(),
            process_hardening: ProcessHardeningReport::default(),
            security_posture: SecurityPosture::detect(false, false, false),
            store_open: StoreOpenProfile::default(),
            store: store_report,
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"security_posture\""));
        assert!(!json.contains("clipboard_content"));
    }
}
