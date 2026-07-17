//! Injectable monotonic wall-clock boundary for deterministic policy tests.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
    }
}

#[derive(Clone, Debug, Default)]
pub struct ManualClock {
    now_ms: Arc<AtomicU64>,
}

impl ManualClock {
    pub fn new(now_ms: u64) -> Self {
        Self {
            now_ms: Arc::new(AtomicU64::new(now_ms)),
        }
    }

    pub fn set(&self, now_ms: u64) {
        self.now_ms.store(now_ms, Ordering::Release);
    }

    pub fn advance(&self, delta_ms: u64) -> u64 {
        self.now_ms
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                Some(current.saturating_add(delta_ms))
            })
            .unwrap_or_else(|current| current)
            .saturating_add(delta_ms)
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.now_ms.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Deadline(u64);

impl Deadline {
    pub fn after(clock: &impl Clock, duration_ms: u64) -> Self {
        Self(clock.now_ms().saturating_add(duration_ms))
    }

    pub fn is_elapsed(self, clock: &impl Clock) -> bool {
        clock.now_ms() >= self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_clock_makes_deadlines_exact_and_instant() {
        let clock = ManualClock::new(1_000);
        let deadline = Deadline::after(&clock, 250);
        assert!(!deadline.is_elapsed(&clock));
        clock.advance(249);
        assert!(!deadline.is_elapsed(&clock));
        clock.advance(1);
        assert!(deadline.is_elapsed(&clock));
    }
}
