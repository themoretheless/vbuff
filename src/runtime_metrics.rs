//! Bounded, content-free runtime metrics retained only in memory.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossbeam_queue::ArrayQueue;
use serde::Serialize;
use vbuff_core::capture::CaptureOutcome;
use vbuff_types::CaptureHealth;

const RING_CAPACITY: usize = 256;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct RuntimeSnapshot {
    pub timestamp_ms: u64,
    pub health: CaptureHealth,
    pub poll_interval_ms: u64,
    pub last_capture_ms: Option<u64>,
    pub captured: u64,
    pub dropped: BTreeMap<String, u64>,
    pub budget_trips: u64,
    pub write_queue_depth: u64,
    pub latency_us: BTreeMap<String, LatencyHistogram>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct LatencyHistogram {
    pub buckets: [u64; 8],
}

impl LatencyHistogram {
    fn observe(&mut self, latency: Duration) {
        const LIMITS_US: [u64; 7] = [100, 500, 1_000, 5_000, 10_000, 50_000, 250_000];
        let micros = latency.as_micros().min(u128::from(u64::MAX)) as u64;
        let bucket = LIMITS_US
            .iter()
            .position(|limit| micros <= *limit)
            .unwrap_or(LIMITS_US.len());
        self.buckets[bucket] = self.buckets[bucket].saturating_add(1);
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RuntimeMetrics {
    current: Arc<Mutex<RuntimeSnapshot>>,
    ring: Arc<ArrayQueue<RuntimeSnapshot>>,
}

impl Default for RuntimeMetrics {
    fn default() -> Self {
        Self {
            current: Arc::new(Mutex::new(RuntimeSnapshot {
                timestamp_ms: unix_time_ms(),
                health: CaptureHealth::Starting,
                poll_interval_ms: 0,
                last_capture_ms: None,
                captured: 0,
                dropped: BTreeMap::new(),
                budget_trips: 0,
                write_queue_depth: 0,
                latency_us: BTreeMap::new(),
            })),
            ring: Arc::new(ArrayQueue::new(RING_CAPACITY)),
        }
    }
}

impl RuntimeMetrics {
    pub(crate) fn health(&self, health: CaptureHealth) {
        self.update(|snapshot| snapshot.health = health);
    }

    pub(crate) fn poll_interval(&self, interval: Duration) {
        self.update(|snapshot| {
            snapshot.poll_interval_ms = interval.as_millis().min(u64::MAX as u128) as u64;
        });
    }

    pub(crate) fn outcome(&self, outcome: CaptureOutcome, count: u64) {
        self.update(|snapshot| match outcome {
            CaptureOutcome::Captured => {
                snapshot.captured = snapshot.captured.saturating_add(count);
                snapshot.last_capture_ms = Some(unix_time_ms());
            }
            CaptureOutcome::Dropped(reason) => {
                let dropped = snapshot.dropped.entry(reason.as_str().into()).or_default();
                *dropped = dropped.saturating_add(count);
            }
        });
    }

    pub(crate) fn budget_trip(&self) {
        self.update(|snapshot| {
            snapshot.budget_trips = snapshot.budget_trips.saturating_add(1);
        });
    }

    pub(crate) fn write_queue_depth(&self, depth: u64) {
        self.update(|snapshot| snapshot.write_queue_depth = depth);
    }

    pub(crate) fn latency(&self, operation: &'static str, latency: Duration) {
        self.update(|snapshot| {
            snapshot
                .latency_us
                .entry(operation.into())
                .or_default()
                .observe(latency);
        });
    }

    pub(crate) fn snapshot(&self) -> Option<RuntimeSnapshot> {
        self.current.lock().ok().map(|snapshot| snapshot.clone())
    }

    pub(crate) fn install_panic_hook(&self, path: PathBuf) {
        let metrics = self.clone();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if let Err(error) = metrics.dump(&path) {
                eprintln!("vbuff could not dump crash metrics: {error}");
            }
            previous(info);
        }));
    }

    fn update(&self, update: impl FnOnce(&mut RuntimeSnapshot)) {
        let Ok(mut current) = self.current.lock() else {
            return;
        };
        update(&mut current);
        current.timestamp_ms = unix_time_ms();
        let snapshot = current.clone();
        drop(current);
        self.ring.force_push(snapshot);
    }

    fn dump(&self, path: &Path) -> std::io::Result<()> {
        let mut snapshots = Vec::with_capacity(self.ring.len());
        while let Some(snapshot) = self.ring.pop() {
            snapshots.push(snapshot);
        }
        let bytes = serde_json::to_vec_pretty(&snapshots)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        atomic_write(path, &bytes)
    }
}

pub(crate) fn crash_metrics_path() -> anyhow::Result<PathBuf> {
    let data = dirs::data_dir().ok_or_else(|| anyhow::anyhow!("data directory unavailable"))?;
    Ok(data.join("vbuff").join("crash-metrics.json"))
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let mut file = atomic_write_file::AtomicWriteFile::open(path)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.commit()
}

pub(crate) fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_ring_is_bounded_and_histograms_are_content_free() {
        let metrics = RuntimeMetrics::default();
        for millis in 0..(RING_CAPACITY + 20) {
            metrics.poll_interval(Duration::from_millis(millis as u64));
        }
        metrics.latency("capture_insert", Duration::from_millis(3));

        assert_eq!(metrics.ring.len(), RING_CAPACITY);
        let snapshot = metrics.snapshot().unwrap();
        assert_eq!(snapshot.latency_us["capture_insert"].buckets[3], 1);
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(!json.contains("clipboard_content"));
    }
}
