use std::fmt;

use serde::{Deserialize, Serialize};
use vbuff_types::ContentKind;

use super::IntegrationContractError;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LauncherClient {
    Raycast,
    Alfred,
    Rofi,
    Dmenu,
    Fzf,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LauncherRequest {
    pub client: LauncherClient,
    pub query: String,
    pub limit: u16,
}

impl fmt::Debug for LauncherRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LauncherRequest")
            .field("client", &self.client)
            .field(
                "query",
                &format_args!("[redacted; {} bytes]", self.query.len()),
            )
            .field("limit", &self.limit)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct LauncherCandidate {
    pub clip_id: String,
    pub signals: LauncherRankSignals,
}

impl fmt::Debug for LauncherCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LauncherCandidate")
            .field("clip_id", &"[redacted]")
            .field("signals", &self.signals)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherRankedResult {
    pub clip_id: String,
    pub score: i64,
    pub rank: u16,
}

impl fmt::Debug for LauncherRankedResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LauncherRankedResult")
            .field("clip_id", &"[redacted]")
            .field("score", &self.score)
            .field("rank", &self.rank)
            .finish()
    }
}

pub fn rank_launcher_candidates(
    request: &LauncherRequest,
    mut candidates: Vec<LauncherCandidate>,
) -> Result<Vec<LauncherRankedResult>, IntegrationContractError> {
    if request.query.len() > 4_096
        || request.query.contains('\0')
        || !(1..=100).contains(&request.limit)
        || candidates.len() > 10_000
        || candidates.iter().any(|candidate| {
            candidate.clip_id.is_empty()
                || candidate.clip_id.len() > 128
                || !candidate
                    .clip_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        })
    {
        return Err(IntegrationContractError::InvalidField);
    }
    candidates.sort_by(|left, right| {
        right
            .signals
            .score()
            .cmp(&left.signals.score())
            .then_with(|| left.clip_id.cmp(&right.clip_id))
    });
    Ok(candidates
        .into_iter()
        .take(usize::from(request.limit))
        .enumerate()
        .map(|(index, candidate)| LauncherRankedResult {
            clip_id: candidate.clip_id,
            score: candidate.signals.score(),
            rank: (index + 1) as u16,
        })
        .collect())
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

    #[test]
    fn launcher_endpoint_is_stable_bounded_and_redacted() {
        let request = LauncherRequest {
            client: LauncherClient::Raycast,
            query: String::new(),
            limit: 1,
        };
        let ranked = rank_launcher_candidates(
            &request,
            vec![
                LauncherCandidate {
                    clip_id: "older".into(),
                    signals: LauncherRankSignals {
                        lexical_score: 1,
                        frecency_score: 1,
                        age_seconds: 1_000,
                        origin_is_remote: false,
                        kind: ContentKind::Text,
                    },
                },
                LauncherCandidate {
                    clip_id: "fresh".into(),
                    signals: LauncherRankSignals {
                        lexical_score: 2,
                        frecency_score: 2,
                        age_seconds: 1,
                        origin_is_remote: true,
                        kind: ContentKind::Text,
                    },
                },
            ],
        )
        .unwrap();
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].clip_id, "fresh");
        assert!(!format!("{:?}", ranked[0]).contains("fresh"));
        assert!(!format!("{request:?}").contains("private"));
    }
}
