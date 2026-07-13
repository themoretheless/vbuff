use std::collections::VecDeque;
use std::time::Duration;

const FTS_ROW_THRESHOLD: usize = 250;
const LIKE_P95_BUDGET: Duration = Duration::from_millis(8);
const LATENCY_WINDOW: usize = 32;

#[derive(Debug, Default)]
pub(crate) struct SearchPlanner {
    like_latencies: VecDeque<Duration>,
    latency_promoted: bool,
}

impl SearchPlanner {
    pub(crate) fn use_fts(&self, row_count: usize, query: &str) -> bool {
        query.chars().count() >= 3 && (row_count >= FTS_ROW_THRESHOLD || self.latency_promoted)
    }

    pub(crate) fn record_like(&mut self, latency: Duration) {
        if self.like_latencies.len() == LATENCY_WINDOW {
            self.like_latencies.pop_front();
        }
        self.like_latencies.push_back(latency);
        let mut sorted = self.like_latencies.iter().copied().collect::<Vec<_>>();
        sorted.sort_unstable();
        let index = (sorted.len().saturating_sub(1) * 95) / 100;
        self.latency_promoted = sorted
            .get(index)
            .is_some_and(|latency| *latency > LIKE_P95_BUDGET);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ParsedQuery {
    pub text: String,
    pub facets: Vec<(String, String)>,
}

pub(crate) fn parse_query(query: &str) -> ParsedQuery {
    let mut text = Vec::new();
    let mut facets = Vec::new();
    for token in query.split_whitespace() {
        match token.split_once(':') {
            Some((key @ ("host" | "color" | "lang" | "iso_date"), value)) if !value.is_empty() => {
                facets.push((key.into(), value.to_lowercase()));
            }
            _ => text.push(token),
        }
    }
    ParsedQuery {
        text: text.join(" "),
        facets,
    }
}

pub(crate) fn fts_literal(query: &str) -> String {
    format!("\"{}\"", query.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planner_promotes_large_histories_and_slow_like() {
        let mut planner = SearchPlanner::default();
        assert!(!planner.use_fts(20, "hello"));
        assert!(planner.use_fts(1_000, "hello"));
        planner.record_like(Duration::from_millis(20));
        assert!(planner.use_fts(20, "hello"));
        assert!(!planner.use_fts(1_000, "hi"));
    }

    #[test]
    fn parser_separates_supported_facets() {
        assert_eq!(
            parse_query("sqlite host:docs.rs lang:rust"),
            ParsedQuery {
                text: "sqlite".into(),
                facets: vec![
                    ("host".into(), "docs.rs".into()),
                    ("lang".into(), "rust".into())
                ],
            }
        );
    }
}
