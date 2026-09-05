//! Machine-readable startup and store diagnostics.

use serde::Serialize;
use vbuff_platform::lifecycle::SessionContext;
use vbuff_platform::{ProcessHardeningReport, SecurityPosture};
use vbuff_store::{Store, StoreDoctorReport, StoreOpenProfile};
use vbuff_types::SecurityPostureLevel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DoctorFormat {
    Human,
    Json,
}

#[derive(Serialize)]
struct DoctorOutput {
    ok: bool,
    capture_allowed: bool,
    required_capabilities_ok: bool,
    store_present: bool,
    version: &'static str,
    storage_backend: String,
    target_os: &'static str,
    session: SessionContext,
    process_hardening: ProcessHardeningReport,
    security_posture: SecurityPosture,
    store_open: StoreOpenProfile,
    store: StoreDoctorReport,
}

/// Health is the store plus the one shared security verdict
/// ([`SecurityPosture::level`]), so `doctor` can never call a machine healthy
/// while the GUI badge reports the same posture as degraded.
fn doctor_ok(store_present: bool, store: &StoreDoctorReport, posture: &SecurityPosture) -> bool {
    store_present && store.is_healthy() && posture.level() == SecurityPostureLevel::Protected
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
    let mut store_present = path.exists();
    let (store_report, store_open) = if let Some(json) =
        crate::single_instance::request_history(crate::single_instance::HistoryRequest::Doctor)?
    {
        // The resident can use a different backend and database path.
        store_present = true;
        (serde_json::from_str(&json)?, StoreOpenProfile::default())
    } else {
        let (store, profile) = if store_present {
            Store::open_read_only_profiled(&path)?
        } else {
            (Store::open_in_memory()?, StoreOpenProfile::default())
        };
        (store.doctor()?, profile)
    };
    // `vbuff doctor` is its own short-lived process, so it takes its own
    // snapshot of its own environment - that is the point of running it. What
    // it must not do is take two: the posture below and the `session` field
    // printed to the user are derived from this single value.
    let session = SessionContext::current();
    let security_posture = SecurityPosture::detect(
        session,
        strict_mode,
        process_hardening.core_dumps_blocked,
        process_hardening.ptrace_blocked,
    );
    let output = DoctorOutput {
        ok: doctor_ok(store_present, &store_report, &security_posture),
        capture_allowed: security_posture.strict_allows_capture(),
        required_capabilities_ok: security_posture.required_capabilities_satisfied(),
        store_present,
        version: env!("CARGO_PKG_VERSION"),
        storage_backend: store_report.backend.clone(),
        target_os: std::env::consts::OS,
        session: session.clone(),
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
                "store backend: {}; present: {}; schema: {}/{}; rows: {}; Search projection healthy: {}",
                output.storage_backend,
                output.store_present,
                output.store.schema_version,
                output.store.expected_schema_version,
                output.store.clip_rows,
                output.store.search.is_healthy()
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
            if !output.ok {
                if !output.store_present {
                    println!("problem: store is missing at the default location");
                } else if !output.store.is_healthy() {
                    println!("problem: store failed its health check");
                }
                for capability in output.security_posture.failing_required() {
                    println!("problem: {} — {}", capability.feature, capability.detail);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_ok_is_reachable_with_satisfied_required_capabilities() {
        let store = Store::open_in_memory().unwrap();
        let store_report = store.doctor().unwrap();
        assert!(store_report.is_healthy());

        let session = SessionContext::current();
        let satisfied = SecurityPosture::detect(session, false, true, true);
        assert!(doctor_ok(true, &store_report, &satisfied));

        let failing = SecurityPosture::detect(session, false, false, true);
        assert!(!doctor_ok(true, &store_report, &failing));
        assert!(!doctor_ok(false, &store_report, &satisfied));
    }

    #[test]
    fn doctor_output_schema_is_stable_and_content_free() {
        let store = Store::open_in_memory().unwrap();
        let store_report = store.doctor().unwrap();
        let session = SessionContext::current();
        let security_posture = SecurityPosture::detect(session, false, false, false);
        let output = DoctorOutput {
            ok: doctor_ok(true, &store_report, &security_posture),
            capture_allowed: true,
            required_capabilities_ok: security_posture.required_capabilities_satisfied(),
            store_present: true,
            version: "test",
            storage_backend: store_report.backend.clone(),
            target_os: "test",
            session: session.clone(),
            process_hardening: ProcessHardeningReport::default(),
            security_posture,
            store_open: StoreOpenProfile::default(),
            store: store_report,
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"security_posture\""));
        assert!(json.contains("\"required_capabilities_ok\""));
        assert!(json.contains("\"severity\""));
        assert!(!json.contains("clipboard_content"));
    }

    /// The reported session and the reported capabilities come from the same
    /// snapshot, so `doctor` cannot print a non-Wayland session next to a
    /// Wayland capability list (or the reverse) on any host.
    #[test]
    fn reported_session_and_capabilities_share_one_snapshot() {
        let session = SessionContext::current();
        let posture = SecurityPosture::detect(session, false, false, false);
        let wayland_rows = posture
            .capabilities
            .iter()
            .any(|capability| capability.feature.starts_with("wayland_"));
        assert_eq!(
            wayland_rows,
            session.display_server == vbuff_platform::lifecycle::DisplayServer::Wayland
        );
    }
}
