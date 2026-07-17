//! Deterministic release SLO evaluation with explicit unknown states.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SloState {
    Met,
    Breached,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SloBudget {
    pub max_lost_captures: u64,
    pub max_search_p99_us: u64,
    pub max_idle_cpu_basis_points: u32,
    pub max_login_ready_ms: u64,
}

impl Default for SloBudget {
    fn default() -> Self {
        Self {
            max_lost_captures: 0,
            max_search_p99_us: 16_000,
            max_idle_cpu_basis_points: 50,
            max_login_ready_ms: 1_500,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SloSample {
    pub lost_captures: Option<u64>,
    pub search_latencies_us: Vec<u64>,
    pub idle_cpu_basis_points: Option<u32>,
    pub login_ready_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SloReport {
    pub zero_loss: SloState,
    pub search_latency: SloState,
    pub idle_cpu: SloState,
    pub login_ready: SloState,
    pub search_p99_us: Option<u64>,
}

impl SloBudget {
    pub fn evaluate(&self, sample: &SloSample) -> SloReport {
        let search_p99_us = percentile_99(&sample.search_latencies_us);
        SloReport {
            zero_loss: sample
                .lost_captures
                .map(|value| state(value <= self.max_lost_captures))
                .unwrap_or(SloState::Unknown),
            search_latency: search_p99_us
                .map(|value| state(value <= self.max_search_p99_us))
                .unwrap_or(SloState::Unknown),
            idle_cpu: sample
                .idle_cpu_basis_points
                .map(|value| state(value <= self.max_idle_cpu_basis_points))
                .unwrap_or(SloState::Unknown),
            login_ready: sample
                .login_ready_ms
                .map(|value| state(value <= self.max_login_ready_ms))
                .unwrap_or(SloState::Unknown),
            search_p99_us,
        }
    }
}

fn state(met: bool) -> SloState {
    if met {
        SloState::Met
    } else {
        SloState::Breached
    }
}

fn percentile_99(values: &[u64]) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let index = (sorted.len() * 99).div_ceil(100).saturating_sub(1);
    sorted.get(index).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluation_never_turns_missing_measurement_into_green() {
        let report = SloBudget::default().evaluate(&SloSample::default());
        assert_eq!(report.zero_loss, SloState::Unknown);
        assert_eq!(report.search_latency, SloState::Unknown);
        assert_eq!(report.idle_cpu, SloState::Unknown);

        let report = SloBudget::default().evaluate(&SloSample {
            lost_captures: Some(1),
            search_latencies_us: vec![1_000, 2_000, 30_000],
            idle_cpu_basis_points: Some(10),
            login_ready_ms: Some(2_000),
        });
        assert_eq!(report.zero_loss, SloState::Breached);
        assert_eq!(report.search_latency, SloState::Breached);
        assert_eq!(report.login_ready, SloState::Breached);
    }
}
