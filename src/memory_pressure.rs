//! Process-memory sampling translated into core resource policy.

use vbuff_core::reliability::{MemoryPressure, MemoryPressurePolicy, MemoryResponse};

use crate::config::Config;

pub(crate) fn current(config: &Config) -> MemoryPressure {
    let Some(usage) = memory_stats::memory_stats() else {
        return MemoryPressure::Normal;
    };
    let soft = config.memory_soft_limit_mb.saturating_mul(1024 * 1024);
    let hard = config
        .memory_hard_limit_mb
        .max(config.memory_soft_limit_mb)
        .saturating_mul(1024 * 1024);
    if usage.physical_mem >= hard {
        MemoryPressure::Critical
    } else if usage.physical_mem >= soft {
        MemoryPressure::Elevated
    } else {
        MemoryPressure::Normal
    }
}

pub(crate) fn response(config: &Config) -> MemoryResponse {
    MemoryPressurePolicy::new(
        64 * 1024 * 1024,
        config.max_history.clamp(1, 1_000),
        config.capture_hard_limit_bytes,
    )
    .response(current(config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_policy_always_preserves_at_least_one_hot_row() {
        let config = Config {
            max_history: 0,
            ..Config::default()
        };
        assert!(response(&config).hot_history_limit >= 1);
    }
}
