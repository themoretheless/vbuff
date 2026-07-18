use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use vbuff_types::ContentKind;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum IntegrationContractError {
    #[error("integration field is invalid")]
    InvalidField,
    #[error("integration request has expired")]
    Expired,
    #[error("integration request is not scoped to one recipient")]
    InvalidRecipient,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryQuery {
    pub query: String,
    pub limit: u16,
    pub include_explanation: bool,
}

impl fmt::Debug for HistoryQuery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HistoryQuery")
            .field(
                "query",
                &format_args!("[redacted; {} bytes]", self.query.len()),
            )
            .field("limit", &self.limit)
            .field("include_explanation", &self.include_explanation)
            .finish()
    }
}

impl HistoryQuery {
    pub fn validate(&self) -> Result<(), IntegrationContractError> {
        if self.query.trim().is_empty()
            || self.query.len() > 4_096
            || self.query.chars().any(|character| character == '\0')
            || !(1..=512).contains(&self.limit)
        {
            return Err(IntegrationContractError::InvalidField);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherRankSignals {
    pub lexical_score: u16,
    pub frecency_score: u16,
    pub age_seconds: u64,
    pub origin_is_remote: bool,
    pub kind: ContentKind,
}

impl LauncherRankSignals {
    pub fn score(self) -> i64 {
        let recency = 10_000_i64.saturating_sub((self.age_seconds / 60).min(10_000) as i64);
        let remote_freshness = i64::from(self.origin_is_remote && self.age_seconds <= 300) * 1_500;
        let kind_bias = match self.kind {
            ContentKind::Text | ContentKind::Code | ContentKind::Url => 250,
            _ => 0,
        };
        i64::from(self.lexical_score) * 16
            + i64::from(self.frecency_score) * 8
            + recency
            + remote_freshness
            + kind_bias
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_remote_results_get_a_bounded_launcher_boost() {
        let base = LauncherRankSignals {
            lexical_score: 100,
            frecency_score: 50,
            age_seconds: 120,
            origin_is_remote: false,
            kind: ContentKind::Text,
        };
        assert!(
            LauncherRankSignals {
                origin_is_remote: true,
                ..base
            }
            .score()
                > base.score()
        );
        assert!(
            HistoryQuery {
                query: "meeting link".into(),
                limit: 20,
                include_explanation: true
            }
            .validate()
            .is_ok()
        );
        let query = HistoryQuery {
            query: "private medical phrase".into(),
            limit: 20,
            include_explanation: false,
        };
        assert!(!format!("{query:?}").contains("medical"));
    }
}
