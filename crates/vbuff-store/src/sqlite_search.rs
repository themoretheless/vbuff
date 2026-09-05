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

    // ---------------------------------------------------------------------
    // Characterization tests for the *current* store grammar.
    //
    // These pin today's behaviour, warts included, so the planned unification
    // with the core `parse_natural_query` grammar has a baseline to diff
    // against. They are deliberately descriptive, not aspirational: a failure
    // here means the grammar changed, not necessarily that it broke.
    // ---------------------------------------------------------------------

    /// The store vocabulary is exactly four keys. Every other `key:value`
    /// token -- including the whole core grammar (`app`, `kind`, `tag`,
    /// `device`, `before`, `after`) and the indexer's own
    /// `has_payment_number` facet -- is silently swallowed into the free-text
    /// tier instead of being rejected.
    #[test]
    fn parser_vocabulary_is_exactly_four_keys() {
        for key in ["host", "color", "lang", "iso_date"] {
            let parsed = parse_query(&format!("{key}:value"));
            assert_eq!(
                parsed.facets,
                vec![(key.to_string(), "value".to_string())],
                "{key} should be a facet"
            );
            assert!(parsed.text.is_empty(), "{key} leaked into the text tier");
        }
        for key in [
            "app",
            "kind",
            "tag",
            "device",
            "before",
            "after",
            "has_payment_number",
            "isodate",
            "iso-date",
            "site",
        ] {
            let parsed = parse_query(&format!("{key}:value"));
            assert!(
                parsed.facets.is_empty(),
                "{key} unexpectedly became a facet"
            );
            assert_eq!(
                parsed.text,
                format!("{key}:value"),
                "{key} should degrade to literal text"
            );
        }
    }

    /// Facet *keys* are matched case-sensitively; facet *values* are
    /// lowercased with the Unicode-aware `to_lowercase`.
    #[test]
    fn parser_keys_are_case_sensitive_and_values_are_lowercased() {
        assert_eq!(
            parse_query("host:DOCS.RS"),
            ParsedQuery {
                text: String::new(),
                facets: vec![("host".into(), "docs.rs".into())],
            }
        );
        assert_eq!(
            parse_query("HOST:docs.rs"),
            ParsedQuery {
                text: "HOST:docs.rs".into(),
                facets: Vec::new(),
            }
        );
        assert_eq!(
            parse_query("Host:docs.rs"),
            ParsedQuery {
                text: "Host:docs.rs".into(),
                facets: Vec::new(),
            }
        );
    }

    /// Non-ASCII values survive tokenization and are lowercased. Note the
    /// indexer stores IDNA/punycode hosts, so a Cyrillic `host:` value can
    /// never match a stored URL facet -- see the store-side test.
    #[test]
    fn parser_lowercases_non_ascii_values() {
        assert_eq!(
            parse_query("host:ПРИМЕР.РФ"),
            ParsedQuery {
                text: String::new(),
                facets: vec![("host".into(), "пример.рф".into())],
            }
        );
        assert_eq!(
            parse_query("Straße"),
            ParsedQuery {
                text: "Straße".into(),
                facets: Vec::new(),
            }
        );
    }

    /// There is no quoting layer at all: `split_whitespace` cuts phrases in
    /// half and the quote characters themselves stay in the tokens.
    #[test]
    fn parser_has_no_quoting_support() {
        assert_eq!(
            parse_query("host:\"two words\""),
            ParsedQuery {
                text: "words\"".into(),
                facets: vec![("host".into(), "\"two".into())],
            }
        );
        // A foreign facet keeps its quotes verbatim, so the LIKE/FTS tier ends
        // up searching for the literal string `app:"two words"`.
        assert_eq!(
            parse_query("app:\"two words\""),
            ParsedQuery {
                text: "app:\"two words\"".into(),
                facets: Vec::new(),
            }
        );
        // Even a plain quoted phrase stays quoted in the text tier.
        assert_eq!(
            parse_query("\"two words\""),
            ParsedQuery {
                text: "\"two words\"".into(),
                facets: Vec::new(),
            }
        );
    }

    /// An empty value disables the facet arm, so the raw token (colon and
    /// all) is searched as text.
    #[test]
    fn parser_sends_empty_values_to_the_text_tier() {
        assert_eq!(
            parse_query("host:"),
            ParsedQuery {
                text: "host:".into(),
                facets: Vec::new(),
            }
        );
        assert_eq!(
            parse_query(":docs.rs"),
            ParsedQuery {
                text: ":docs.rs".into(),
                facets: Vec::new(),
            }
        );
        assert_eq!(parse_query(""), ParsedQuery::default());
        assert_eq!(parse_query("   \t\n "), ParsedQuery::default());
    }

    /// Only the first colon separates key from value, so ports, times and
    /// namespaced values all end up inside the value.
    #[test]
    fn parser_splits_on_the_first_colon_only() {
        assert_eq!(
            parse_query("host:example.com:8443"),
            ParsedQuery {
                text: String::new(),
                facets: vec![("host".into(), "example.com:8443".into())],
            }
        );
    }

    /// Whitespace is normalized: the text tier is a single-space join of the
    /// surviving tokens, in original order.
    #[test]
    fn parser_collapses_whitespace_in_the_text_tier() {
        assert_eq!(
            parse_query("  alpha \t host:docs.rs \n beta  "),
            ParsedQuery {
                text: "alpha beta".into(),
                facets: vec![("host".into(), "docs.rs".into())],
            }
        );
    }

    /// Repeated keys are not deduplicated or OR-ed; the SQL tier ANDs them,
    /// so two values for one key can never match.
    #[test]
    fn parser_keeps_repeated_facets_verbatim() {
        assert_eq!(
            parse_query("host:a host:b").facets,
            vec![
                ("host".to_string(), "a".to_string()),
                ("host".to_string(), "b".to_string()),
            ]
        );
    }

    /// `fts_literal` wraps the whole text tier in one FTS5 phrase and doubles
    /// embedded double quotes. That neutralizes every FTS5 operator, but it
    /// does nothing about LIKE metacharacters (that is `escape_like`'s job).
    #[test]
    fn fts_literal_quotes_the_query_and_doubles_inner_quotes() {
        assert_eq!(fts_literal("plain"), "\"plain\"");
        assert_eq!(fts_literal("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(fts_literal("\""), "\"\"\"\"");
        assert_eq!(
            fts_literal("a* OR b NEAR/2 c ^d -e"),
            "\"a* OR b NEAR/2 c ^d -e\""
        );
        assert_eq!(fts_literal("100% _x_ c:\\tmp"), "\"100% _x_ c:\\tmp\"");
        // Nothing guards against a token set that tokenizes to nothing; the
        // caller only checks that the text tier is non-empty.
        assert_eq!(fts_literal("***"), "\"***\"");
    }

    /// The FTS/LIKE tier choice counts characters, not bytes, so a two-glyph
    /// multi-byte query stays on the LIKE tier.
    #[test]
    fn planner_counts_characters_not_bytes() {
        let planner = SearchPlanner::default();
        assert!(!planner.use_fts(1_000, "工作"));
        assert!(planner.use_fts(1_000, "工作表"));
    }
}
