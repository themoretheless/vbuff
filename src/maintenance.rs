//! Low-frequency, capture-friendly store maintenance.

use std::time::Duration;

use vbuff_core::capture::{BudgetObservation, SubsystemBudget};

use crate::diagnostics::Diagnostics;
use crate::history::History;

const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(60);
const MAX_MAINTENANCE_INTERVAL: Duration = Duration::from_secs(15 * 60);

pub(crate) fn spawn(history: History, diagnostics: Diagnostics) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut interval = MAINTENANCE_INTERVAL;
        let mut budget =
            SubsystemBudget::new(Duration::from_secs(15 * 60), Duration::from_secs(2), 15);
        loop {
            std::thread::sleep(interval);
            let cpu_started = cpu_time::ThreadTime::now();
            let wall_started = std::time::Instant::now();
            let result = history.maintain_idle();
            diagnostics.latency("store_maintenance", wall_started.elapsed());
            let observation = budget.record(std::time::Instant::now(), cpu_started.elapsed(), 1);
            if observation == BudgetObservation::WithinBudget {
                interval = interval
                    .saturating_sub(MAINTENANCE_INTERVAL)
                    .max(MAINTENANCE_INTERVAL);
            } else {
                interval = (interval * 2).min(MAX_MAINTENANCE_INTERVAL);
                diagnostics.budget_trip();
                tracing::warn!(
                    ?observation,
                    ?interval,
                    "store maintenance exceeded its budget"
                );
            }
            match result {
                Ok(Some(summary))
                    if summary.fts_optimized
                        || summary.fingerprints > 0
                        || summary.embeddings > 0
                        || summary.repaired > 0
                        || summary.quarantined > 0
                        || summary.expired > 0
                        || summary.blobs_collected > 0 =>
                {
                    tracing::debug!(
                        fingerprints = summary.fingerprints,
                        embeddings = summary.embeddings,
                        audited = summary.audited,
                        repaired = summary.repaired,
                        quarantined = summary.quarantined,
                        expired = summary.expired,
                        blobs_collected = summary.blobs_collected,
                        fts_optimized = summary.fts_optimized,
                        "idle store maintenance completed"
                    );
                }
                Ok(Some(_)) | Ok(None) => {}
                Err(error) => tracing::warn!("idle store maintenance failed: {error}"),
            }
        }
    })
}
