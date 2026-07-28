use std::collections::BTreeMap;

use super::{OperationContractError, OperationResult};

const MAX_RATE_LIMIT_TOKENS: usize = 1_024;
const MIN_RATE_LIMIT_WINDOW_MS: u64 = 1_000;
const MAX_RATE_LIMIT_WINDOW_MS: u64 = 24 * 60 * 60 * 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RateLimitKind {
    Read,
    Write,
    Paste,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateLimitPolicy {
    pub window_ms: u64,
    pub reads_per_window: u32,
    pub writes_per_window: u32,
    pub pastes_per_window: u32,
}

impl RateLimitPolicy {
    pub const fn quota(self, kind: RateLimitKind) -> u32 {
        match kind {
            RateLimitKind::Read => self.reads_per_window,
            RateLimitKind::Write => self.writes_per_window,
            RateLimitKind::Paste => self.pastes_per_window,
        }
    }

    pub const fn is_valid(self) -> bool {
        self.window_ms >= MIN_RATE_LIMIT_WINDOW_MS
            && self.window_ms <= MAX_RATE_LIMIT_WINDOW_MS
            && self.reads_per_window > 0
            && self.writes_per_window > 0
            && self.pastes_per_window > 0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TokenWindow {
    window_id: u64,
    counts: BTreeMap<RateLimitKind, u32>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct TokenRateLimiter {
    policy: RateLimitPolicy,
    windows: BTreeMap<[u8; 32], TokenWindow>,
    latest_window_id: Option<u64>,
}

impl std::fmt::Debug for TokenRateLimiter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TokenRateLimiter")
            .field("policy", &self.policy)
            .field("token_count", &self.windows.len())
            .finish()
    }
}

impl TokenRateLimiter {
    pub fn new(policy: RateLimitPolicy) -> OperationResult<Self> {
        if !policy.is_valid() {
            return Err(OperationContractError::Invalid("rate_limit_policy_invalid"));
        }
        Ok(Self {
            policy,
            windows: BTreeMap::new(),
            latest_window_id: None,
        })
    }

    pub fn admit(
        &mut self,
        token_hash: [u8; 32],
        kind: RateLimitKind,
        now_ms: u64,
    ) -> OperationResult<()> {
        if token_hash == [0; 32] {
            return Err(OperationContractError::Invalid(
                "rate_limit_token_hash_invalid",
            ));
        }
        let window_id = now_ms / self.policy.window_ms;
        if self
            .latest_window_id
            .is_some_and(|latest| window_id < latest)
        {
            return Err(OperationContractError::Invalid(
                "rate_limit_clock_moved_backward",
            ));
        }
        self.latest_window_id = Some(window_id);
        if !self.windows.contains_key(&token_hash) {
            self.windows
                .retain(|_, window| window.window_id >= window_id.saturating_sub(1));
        }
        if !self.windows.contains_key(&token_hash) && self.windows.len() >= MAX_RATE_LIMIT_TOKENS {
            return Err(OperationContractError::RateLimited);
        }
        let window = self.windows.entry(token_hash).or_default();
        if window_id < window.window_id {
            return Err(OperationContractError::Invalid(
                "rate_limit_clock_moved_backward",
            ));
        }
        if window.window_id < window_id {
            window.window_id = window_id;
            window.counts.clear();
        }
        let count = window.counts.entry(kind).or_default();
        if *count >= self.policy.quota(kind) {
            return Err(OperationContractError::RateLimited);
        }
        *count += 1;
        Ok(())
    }
}
