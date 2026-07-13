use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollObservation {
    Activity,
    ClipboardChanged,
    Stable,
    MissRisk,
}

/// Activity-sensitive cadence with deterministic bounds.
#[derive(Clone, Debug)]
pub struct AdaptivePollScheduler {
    minimum: Duration,
    maximum: Duration,
    current: Duration,
    active_for: Duration,
    active_until: Option<Instant>,
}

impl AdaptivePollScheduler {
    pub fn new(minimum: Duration, initial: Duration, maximum: Duration) -> Self {
        let minimum = minimum.max(Duration::from_millis(10));
        let maximum = maximum.max(minimum);
        Self {
            minimum,
            maximum,
            current: initial.clamp(minimum, maximum),
            active_for: Duration::from_secs(3),
            active_until: None,
        }
    }

    pub fn observe(&mut self, observation: PollObservation, now: Instant) -> Duration {
        match observation {
            PollObservation::Activity | PollObservation::ClipboardChanged => {
                self.current = self.minimum;
                self.active_until = Some(now + self.active_for);
            }
            PollObservation::MissRisk => {
                self.current = self.minimum;
                self.active_until = Some(now + self.active_for * 2);
            }
            PollObservation::Stable if self.active_until.is_none_or(|until| now >= until) => {
                let grown = self.current + self.current / 2;
                self.current = grown.min(self.maximum);
            }
            PollObservation::Stable => {}
        }
        self.current
    }

    pub fn interval(&self) -> Duration {
        self.current
    }

    pub fn back_off(&mut self) -> Duration {
        self.current = (self.current * 2).min(self.maximum);
        self.current
    }
}

impl Default for AdaptivePollScheduler {
    fn default() -> Self {
        Self::new(
            Duration::from_millis(120),
            Duration::from_millis(300),
            Duration::from_millis(900),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetObservation {
    WithinBudget,
    CpuExceeded,
    WakeupsExceeded,
    BothExceeded,
}

#[derive(Clone, Copy, Debug)]
struct Sample {
    at: Instant,
    cpu: Duration,
    wakeups: u32,
}

/// Rolling tripwire for one subsystem. Callers choose the sampling source.
#[derive(Debug)]
pub struct SubsystemBudget {
    window: Duration,
    max_cpu: Duration,
    max_wakeups: u32,
    samples: VecDeque<Sample>,
}

impl SubsystemBudget {
    pub fn new(window: Duration, max_cpu: Duration, max_wakeups: u32) -> Self {
        Self {
            window,
            max_cpu,
            max_wakeups,
            samples: VecDeque::new(),
        }
    }

    pub fn record(&mut self, at: Instant, cpu: Duration, wakeups: u32) -> BudgetObservation {
        self.samples.push_back(Sample { at, cpu, wakeups });
        while self
            .samples
            .front()
            .is_some_and(|sample| at.saturating_duration_since(sample.at) > self.window)
        {
            self.samples.pop_front();
        }
        let cpu = self
            .samples
            .iter()
            .map(|sample| sample.cpu)
            .sum::<Duration>();
        let wakeups = self
            .samples
            .iter()
            .map(|sample| sample.wakeups)
            .sum::<u32>();
        match (cpu > self.max_cpu, wakeups > self.max_wakeups) {
            (false, false) => BudgetObservation::WithinBudget,
            (true, false) => BudgetObservation::CpuExceeded,
            (false, true) => BudgetObservation::WakeupsExceeded,
            (true, true) => BudgetObservation::BothExceeded,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_is_fast_in_bursts_and_relaxes_when_idle() {
        let start = Instant::now();
        let mut scheduler = AdaptivePollScheduler::default();
        assert_eq!(
            scheduler.observe(PollObservation::ClipboardChanged, start),
            Duration::from_millis(120)
        );
        assert_eq!(
            scheduler.observe(PollObservation::Stable, start + Duration::from_secs(1)),
            Duration::from_millis(120)
        );
        assert_eq!(
            scheduler.observe(PollObservation::Stable, start + Duration::from_secs(4)),
            Duration::from_millis(180)
        );
    }

    #[test]
    fn budget_reports_each_dimension() {
        let start = Instant::now();
        let mut budget =
            SubsystemBudget::new(Duration::from_secs(60), Duration::from_millis(10), 2);
        assert_eq!(
            budget.record(start, Duration::from_millis(2), 1),
            BudgetObservation::WithinBudget
        );
        assert_eq!(
            budget.record(start + Duration::from_secs(1), Duration::from_millis(9), 2),
            BudgetObservation::BothExceeded
        );
    }
}
