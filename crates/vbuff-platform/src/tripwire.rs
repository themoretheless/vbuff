//! Heuristic detector for repeated clipboard reads by another process.

use std::collections::{BTreeMap, VecDeque};

const MAX_OBSERVATIONS: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipboardReadObservation {
    pub process_id: u32,
    pub timestamp_ms: u64,
    pub is_vbuff: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TripwireAlert {
    pub process_id: u32,
    pub reads_in_window: usize,
    pub window_ms: u64,
}

#[derive(Clone, Debug)]
pub struct ScrapeTripwire {
    window_ms: u64,
    threshold: usize,
    observations: VecDeque<ClipboardReadObservation>,
    last_alert_ms: BTreeMap<u32, u64>,
    last_observed_ms: Option<u64>,
}

impl ScrapeTripwire {
    pub fn new(window_ms: u64, threshold: usize) -> Self {
        Self {
            window_ms: window_ms.max(1),
            threshold: threshold.clamp(2, MAX_OBSERVATIONS),
            observations: VecDeque::new(),
            last_alert_ms: BTreeMap::new(),
            last_observed_ms: None,
        }
    }

    pub fn observe(&mut self, observation: ClipboardReadObservation) -> Option<TripwireAlert> {
        if self
            .last_observed_ms
            .is_some_and(|last| observation.timestamp_ms < last)
        {
            self.observations.clear();
            self.last_alert_ms.clear();
        }
        self.last_observed_ms = Some(observation.timestamp_ms);
        let cutoff = observation.timestamp_ms.saturating_sub(self.window_ms);
        while self
            .observations
            .front()
            .is_some_and(|entry| entry.timestamp_ms < cutoff)
        {
            self.observations.pop_front();
        }
        self.last_alert_ms
            .retain(|_, timestamp_ms| *timestamp_ms >= cutoff);
        if observation.is_vbuff || observation.process_id == 0 {
            return None;
        }
        if self.observations.len() == MAX_OBSERVATIONS {
            self.observations.pop_front();
        }
        self.observations.push_back(observation);
        let count = self
            .observations
            .iter()
            .filter(|entry| entry.process_id == observation.process_id)
            .count();
        let recently_alerted = self
            .last_alert_ms
            .get(&observation.process_id)
            .is_some_and(|last| observation.timestamp_ms.saturating_sub(*last) < self.window_ms);
        if count < self.threshold || recently_alerted {
            return None;
        }
        self.last_alert_ms
            .insert(observation.process_id, observation.timestamp_ms);
        Some(TripwireAlert {
            process_id: observation.process_id,
            reads_in_window: count,
            window_ms: self.window_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alert_requires_repeated_external_reads_and_is_rate_limited() {
        let mut detector = ScrapeTripwire::new(1_000, 3);
        for timestamp_ms in [10, 20] {
            assert!(
                detector
                    .observe(ClipboardReadObservation {
                        process_id: 7,
                        timestamp_ms,
                        is_vbuff: false,
                    })
                    .is_none()
            );
        }
        let alert = detector
            .observe(ClipboardReadObservation {
                process_id: 7,
                timestamp_ms: 30,
                is_vbuff: false,
            })
            .unwrap();
        assert_eq!(alert.reads_in_window, 3);
        assert!(
            detector
                .observe(ClipboardReadObservation {
                    process_id: 7,
                    timestamp_ms: 40,
                    is_vbuff: false,
                })
                .is_none()
        );

        assert!(
            detector
                .observe(ClipboardReadObservation {
                    process_id: 7,
                    timestamp_ms: 1,
                    is_vbuff: false,
                })
                .is_none()
        );
        assert_eq!(detector.observations.len(), 1);
    }
}
