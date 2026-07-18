use std::collections::VecDeque;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerState {
    Ac,
    Battery { percent: u8 },
    ThermalThrottled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InferenceBudget {
    pub enabled: bool,
    pub max_input_bytes: usize,
    pub max_queue_depth: usize,
    pub ac_only: bool,
}

impl Default for InferenceBudget {
    fn default() -> Self {
        Self {
            enabled: false,
            max_input_bytes: 256 * 1024,
            max_queue_depth: 64,
            ac_only: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InferenceDecision {
    RunNow,
    Defer,
    RejectDisabled,
    RejectTooLarge,
    RejectQueueFull,
}

impl InferenceBudget {
    pub fn decide(
        self,
        input_bytes: usize,
        queue_depth: usize,
        power: PowerState,
    ) -> InferenceDecision {
        if !self.enabled {
            return InferenceDecision::RejectDisabled;
        }
        if input_bytes > self.max_input_bytes {
            return InferenceDecision::RejectTooLarge;
        }
        if queue_depth >= self.max_queue_depth {
            return InferenceDecision::RejectQueueFull;
        }
        if matches!(power, PowerState::ThermalThrottled)
            || (self.ac_only && !matches!(power, PowerState::Ac))
        {
            return InferenceDecision::Defer;
        }
        InferenceDecision::RunNow
    }
}

#[derive(Clone)]
pub struct InferenceQueue<T> {
    budget: InferenceBudget,
    entries: VecDeque<T>,
}

impl<T> fmt::Debug for InferenceQueue<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InferenceQueue")
            .field("budget", &self.budget)
            .field("queued_items", &self.entries.len())
            .finish()
    }
}

impl<T> InferenceQueue<T> {
    pub fn new(budget: InferenceBudget) -> Self {
        Self {
            budget,
            entries: VecDeque::new(),
        }
    }

    pub fn push(&mut self, value: T, input_bytes: usize, power: PowerState) -> InferenceDecision {
        let decision = self.budget.decide(input_bytes, self.entries.len(), power);
        if matches!(
            decision,
            InferenceDecision::RunNow | InferenceDecision::Defer
        ) {
            self.entries.push_back(value);
        }
        decision
    }

    pub fn pop_ready(&mut self, power: PowerState) -> Option<T> {
        matches!(self.budget.decide(0, 0, power), InferenceDecision::RunNow)
            .then(|| self.entries.pop_front())
            .flatten()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inference_is_off_by_default_and_defers_under_power_pressure() {
        assert_eq!(
            InferenceBudget::default().decide(1, 0, PowerState::Ac),
            InferenceDecision::RejectDisabled
        );
        let budget = InferenceBudget {
            enabled: true,
            max_queue_depth: 1,
            ..InferenceBudget::default()
        };
        let mut queue = InferenceQueue::new(budget);
        assert_eq!(
            queue.push(1, 10, PowerState::Battery { percent: 80 }),
            InferenceDecision::Defer
        );
        assert_eq!(
            queue.push(2, 10, PowerState::Ac),
            InferenceDecision::RejectQueueFull
        );
        assert!(
            queue
                .pop_ready(PowerState::Battery { percent: 80 })
                .is_none()
        );
        assert_eq!(queue.pop_ready(PowerState::Ac), Some(1));
    }
}
