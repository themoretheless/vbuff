//! Atomically published external liveness heartbeat.

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

use crate::diagnostics::Diagnostics;
use crate::runtime_metrics::{RuntimeSnapshot, atomic_write, unix_time_ms};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Serialize)]
struct Heartbeat<'a> {
    timestamp_ms: u64,
    pid: u32,
    capture_state: vbuff_types::CaptureHealth,
    schema_version: i64,
    last_capture_age_ms: Option<u64>,
    poll_interval_ms: u64,
    captured: u64,
    dropped: &'a std::collections::BTreeMap<String, u64>,
    budget_trips: u64,
}

pub(crate) fn spawn(diagnostics: Diagnostics) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let path = match heartbeat_path() {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!("external heartbeat disabled: {error}");
                return;
            }
        };
        loop {
            if let Some(snapshot) = diagnostics.runtime_snapshot()
                && let Err(error) = write_heartbeat(&path, &snapshot)
            {
                tracing::warn!("external heartbeat write failed: {error}");
            }
            std::thread::sleep(HEARTBEAT_INTERVAL);
        }
    })
}

fn heartbeat_path() -> anyhow::Result<PathBuf> {
    let data = dirs::data_dir().ok_or_else(|| anyhow::anyhow!("data directory unavailable"))?;
    Ok(data.join("vbuff").join("heartbeat.json"))
}

fn write_heartbeat(path: &std::path::Path, snapshot: &RuntimeSnapshot) -> anyhow::Result<()> {
    let now = unix_time_ms();
    let payload = Heartbeat {
        timestamp_ms: now,
        pid: std::process::id(),
        capture_state: snapshot.health,
        schema_version: vbuff_store::SCHEMA_VERSION,
        last_capture_age_ms: snapshot
            .last_capture_ms
            .map(|captured| now.saturating_sub(captured)),
        poll_interval_ms: snapshot.poll_interval_ms,
        captured: snapshot.captured,
        dropped: &snapshot.dropped,
        budget_trips: snapshot.budget_trips,
    };
    atomic_write(path, &serde_json::to_vec_pretty(&payload)?)?;
    Ok(())
}
