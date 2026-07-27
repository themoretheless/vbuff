use vbuff_types::Clip;

use super::{ClipTags, NaturalQuery, PasteAffinity, PinnedAliases, QueryPinSet, SearchScopeLock};

const MAX_RECALL_INPUT: usize = 10_000;
const MAX_SEARCHABLE_CHARS: usize = 4_096;
const MAX_COMPLETIONS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchExplanation {
    Text,
    SourceApplication,
    Kind,
    Tag,
    Device,
    Time,
    PinnedAlias,
    TypoCorrection,
    DestinationAffinity,
    QueryPinned,
}

#[derive(Clone, Debug)]
pub struct RecallSearchResult<'a> {
    pub clip: &'a Clip,
    pub score: i64,
    pub explanations: Vec<MatchExplanation>,
    query_pinned: bool,
    source_index: usize,
}

impl RecallSearchResult<'_> {
    pub const fn query_pinned(&self) -> bool {
        self.query_pinned
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RecallSearchContext<'a> {
    pub current_app: Option<&'a str>,
    pub aliases: Option<&'a PinnedAliases>,
    pub affinity: Option<&'a PasteAffinity>,
    pub tags: Option<&'a ClipTags>,
    pub query_pins: Option<&'a QueryPinSet>,
    pub scope: Option<&'a SearchScopeLock>,
}

pub fn search_recall<'a>(
    clips: &'a [Clip],
    query: &NaturalQuery,
    context: RecallSearchContext<'_>,
) -> Vec<RecallSearchResult<'a>> {
    let text_query = query.text.trim().to_lowercase();
    let mut results = clips
        .iter()
        .enumerate()
        .take(MAX_RECALL_INPUT)
        .filter_map(|(source_index, clip)| {
            if !matches_filters(clip, query, context.tags)
                || context
                    .scope
                    .is_some_and(|scope| !scope.matches(clip, context.tags))
            {
                return None;
            }
            let mut explanations = filter_explanations(query);
            let mut score = 0_i64;
            if !text_query.is_empty() {
                if let Some((text_score, explanation)) = score_text(clip, &text_query) {
                    score += text_score;
                    push_once(&mut explanations, explanation);
                } else if let Some(alias_score) = context
                    .aliases
                    .and_then(|aliases| aliases.match_score(clip.id, &text_query))
                {
                    score += alias_score;
                    push_once(&mut explanations, MatchExplanation::PinnedAlias);
                } else if typo_matches(clip, &text_query) {
                    score += 35;
                    push_once(&mut explanations, MatchExplanation::TypoCorrection);
                } else {
                    return None;
                }
            }
            if let (Some(app), Some(affinity)) = (context.current_app, context.affinity) {
                let boost = affinity.boost(app, clip.content_hash);
                if boost > 0 {
                    score += boost;
                    push_once(&mut explanations, MatchExplanation::DestinationAffinity);
                }
            }
            let query_pinned = context
                .query_pins
                .is_some_and(|pins| pins.contains(query.fingerprint(), clip.id));
            if query_pinned {
                score += 500;
                push_once(&mut explanations, MatchExplanation::QueryPinned);
            }
            Some(RecallSearchResult {
                clip,
                score,
                explanations,
                query_pinned,
                source_index,
            })
        })
        .collect::<Vec<_>>();
    results.sort_by(|left, right| {
        right
            .query_pinned
            .cmp(&left.query_pinned)
            .then_with(|| right.clip.pinned.cmp(&left.clip.pinned))
            .then_with(|| right.score.cmp(&left.score))
            .then_with(|| left.source_index.cmp(&right.source_index))
    });
    results
}

fn matches_filters(clip: &Clip, query: &NaturalQuery, tags: Option<&ClipTags>) -> bool {
    query.kind.is_none_or(|kind| clip.meta.kind == kind)
        && query.app.as_deref().is_none_or(|app| {
            clip.meta
                .source_app
                .as_deref()
                .is_some_and(|source| source.to_lowercase().contains(app))
        })
        && query.device.as_deref().is_none_or(|device| {
            clip.meta
                .lineage
                .origin_device
                .as_deref()
                .is_some_and(|source| source.eq_ignore_ascii_case(device))
        })
        && query
            .tag
            .as_deref()
            .is_none_or(|tag| tags.is_some_and(|tags| tags.has_tag(clip.id, tag)))
        && query
            .before
            .is_none_or(|before| clip.meta.created_at < before)
        && query
            .after
            .is_none_or(|after| clip.meta.created_at >= after)
}

fn filter_explanations(query: &NaturalQuery) -> Vec<MatchExplanation> {
    let mut explanations = Vec::with_capacity(6);
    if query.app.is_some() {
        explanations.push(MatchExplanation::SourceApplication);
    }
    if query.kind.is_some() {
        explanations.push(MatchExplanation::Kind);
    }
    if query.tag.is_some() {
        explanations.push(MatchExplanation::Tag);
    }
    if query.device.is_some() {
        explanations.push(MatchExplanation::Device);
    }
    if query.before.is_some() || query.after.is_some() {
        explanations.push(MatchExplanation::Time);
    }
    explanations
}

fn score_text(clip: &Clip, query: &str) -> Option<(i64, MatchExplanation)> {
    if !clip.meta.sensitive
        && let Some(text) = clip.primary_text()
    {
        let bounded = text.chars().take(MAX_SEARCHABLE_CHARS).collect::<String>();
        if let Some(score) = substring_score(&bounded.to_lowercase(), query) {
            return Some((score, MatchExplanation::Text));
        }
    }
    if let Some(app) = &clip.meta.source_app
        && let Some(score) = substring_score(&app.to_lowercase(), query)
    {
        return Some((
            score.saturating_sub(10),
            MatchExplanation::SourceApplication,
        ));
    }
    substring_score(&clip.meta.kind.label().to_lowercase(), query)
        .map(|score| (score.saturating_sub(20), MatchExplanation::Kind))
}

fn substring_score(haystack: &str, query: &str) -> Option<i64> {
    let position = haystack.find(query)?;
    let mut score = 100_i64.saturating_sub(position as i64);
    if position == 0 {
        score += 50;
    } else if haystack[..position]
        .chars()
        .next_back()
        .is_some_and(char::is_whitespace)
    {
        score += 20;
    }
    score -= (haystack.chars().count() / 64) as i64;
    Some(score)
}

fn typo_matches(clip: &Clip, query: &str) -> bool {
    if clip.meta.sensitive
        || query.contains(char::is_whitespace)
        || !(3..=32).contains(&query.chars().count())
    {
        return false;
    }
    clip.primary_text().is_some_and(|text| {
        let bounded = text.chars().take(MAX_SEARCHABLE_CHARS).collect::<String>();
        bounded
            .split(|ch: char| !ch.is_alphanumeric())
            .take(256)
            .filter(|word| word.chars().take(33).count() <= 32)
            .any(|word| edit_distance_at_most_one(&word.to_lowercase(), query))
    })
}

fn edit_distance_at_most_one(left: &str, right: &str) -> bool {
    if left.chars().take(33).count() > 32 || right.chars().take(33).count() > 32 {
        return false;
    }
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    if left.len().abs_diff(right.len()) > 1 {
        return false;
    }
    let mut left_index = 0;
    let mut right_index = 0;
    let mut edits = 0;
    while left_index < left.len() && right_index < right.len() {
        if left[left_index] == right[right_index] {
            left_index += 1;
            right_index += 1;
            continue;
        }
        edits += 1;
        if edits > 1 {
            return false;
        }
        match left.len().cmp(&right.len()) {
            std::cmp::Ordering::Greater => left_index += 1,
            std::cmp::Ordering::Less => right_index += 1,
            std::cmp::Ordering::Equal => {
                left_index += 1;
                right_index += 1;
            }
        }
    }
    edits + usize::from(left_index < left.len() || right_index < right.len()) <= 1
}

fn push_once(output: &mut Vec<MatchExplanation>, explanation: MatchExplanation) {
    if !output.contains(&explanation) {
        output.push(explanation);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissSuggestion {
    BroadenTime,
    ClearApplication,
    ClearKind,
    ClearTag,
    IncludeArchive,
    SearchAllDevices,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchMiss {
    pub suggestions: Vec<MissSuggestion>,
}

impl SearchMiss {
    pub fn for_query(
        query: &NaturalQuery,
        archive_available: bool,
        remote_available: bool,
    ) -> Self {
        let mut suggestions = Vec::with_capacity(6);
        if query.before.is_some() || query.after.is_some() {
            suggestions.push(MissSuggestion::BroadenTime);
        }
        if query.app.is_some() {
            suggestions.push(MissSuggestion::ClearApplication);
        }
        if query.kind.is_some() {
            suggestions.push(MissSuggestion::ClearKind);
        }
        if query.tag.is_some() {
            suggestions.push(MissSuggestion::ClearTag);
        }
        if archive_available {
            suggestions.push(MissSuggestion::IncludeArchive);
        }
        if remote_available {
            suggestions.push(MissSuggestion::SearchAllDevices);
        }
        Self { suggestions }
    }
}

pub fn complete_query(input: &str) -> Vec<String> {
    if input.len() > 4 * 1_024 {
        return Vec::new();
    }
    let token = input.split_whitespace().next_back().unwrap_or_default();
    let lower = token.to_ascii_lowercase();
    let candidates: &[&str] = if lower.starts_with("kind:") {
        &[
            "kind:text",
            "kind:url",
            "kind:image",
            "kind:code",
            "kind:file",
            "kind:color",
        ]
    } else {
        &["app:", "kind:", "tag:", "device:", "before:", "after:"]
    };
    candidates
        .iter()
        .filter(|candidate| candidate.starts_with(&lower))
        .take(MAX_COMPLETIONS)
        .map(|candidate| (*candidate).to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone as _, Utc};
    use vbuff_types::{ClipId, ClipMeta, ContentKind, Flavor};

    use super::*;
    use crate::recall::parse_natural_query;

    fn clip(text: &str, app: &str, kind: ContentKind, at: chrono::DateTime<Utc>) -> Clip {
        Clip {
            id: ClipId::new(),
            flavors: vec![Flavor::inline("text/plain", text.as_bytes().to_vec())],
            content_hash: *blake3::hash(text.as_bytes()).as_bytes(),
            meta: ClipMeta {
                created_at: at,
                ..ClipMeta::now(kind, text.len() as u64, Some(app.into()))
            },
            pinned: false,
            favorite: false,
        }
    }

    #[test]
    fn structured_search_explains_filters_and_context_boost() {
        let now = Utc.with_ymd_and_hms(2026, 7, 21, 12, 0, 0).unwrap();
        let clips = vec![
            clip("release notes", "Chrome", ContentKind::Url, now),
            clip(
                "release draft",
                "Editor",
                ContentKind::Text,
                now - Duration::days(10),
            ),
        ];
        let query = parse_natural_query("release urls from chrome last week", now).unwrap();
        let mut affinity = PasteAffinity::default();
        assert!(affinity.record("terminal", clips[0].content_hash));
        let results = search_recall(
            &clips,
            &query,
            RecallSearchContext {
                current_app: Some("terminal"),
                affinity: Some(&affinity),
                ..RecallSearchContext::default()
            },
        );
        assert_eq!(results.len(), 1);
        assert!(results[0].explanations.contains(&MatchExplanation::Text));
        assert!(
            results[0]
                .explanations
                .contains(&MatchExplanation::SourceApplication)
        );
        assert!(
            results[0]
                .explanations
                .contains(&MatchExplanation::DestinationAffinity)
        );
    }

    #[test]
    fn short_typo_and_alias_are_recoverable_but_sensitive_text_is_not_searched() {
        let now = Utc.timestamp_opt(1_000, 0).unwrap();
        let mut ordinary = clip("deployment command", "terminal", ContentKind::Code, now);
        ordinary.pinned = true;
        let mut sensitive = clip("secret phrase", "vault", ContentKind::Text, now);
        sensitive.meta.sensitive = true;
        let clips = vec![ordinary.clone(), sensitive];
        let typo = parse_natural_query("deplyment", now).unwrap();
        assert_eq!(
            search_recall(&clips, &typo, RecallSearchContext::default()).len(),
            1
        );
        let secret = parse_natural_query("secret", now).unwrap();
        assert!(search_recall(&clips, &secret, RecallSearchContext::default()).is_empty());

        let mut aliases = PinnedAliases::default();
        assert!(aliases.add(ordinary.id, true, "ship"));
        let alias = parse_natural_query("ship", now).unwrap();
        let hits = search_recall(
            &clips,
            &alias,
            RecallSearchContext {
                aliases: Some(&aliases),
                ..RecallSearchContext::default()
            },
        );
        assert_eq!(hits.len(), 1);
        assert!(
            hits[0]
                .explanations
                .contains(&MatchExplanation::PinnedAlias)
        );
    }

    #[test]
    fn miss_suggestions_and_completions_follow_active_constraints() {
        let now = Utc.timestamp_opt(1_000, 0).unwrap();
        let query = parse_natural_query("kind:url app:browser after:last-2h", now).unwrap();
        let miss = SearchMiss::for_query(&query, true, true);
        assert!(miss.suggestions.contains(&MissSuggestion::BroadenTime));
        assert!(miss.suggestions.contains(&MissSuggestion::IncludeArchive));
        assert_eq!(complete_query("ki"), vec!["kind:"]);
        assert!(complete_query("kind:u").contains(&"kind:url".to_owned()));
    }
}
