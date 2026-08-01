use vbuff_types::{Clip, ContentKind};

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
    // Deliberate UX subset: only the kinds users plausibly filter by are
    // completed (Rtf, Html and Other stay out, though the parser accepts
    // them). The spelling comes from the canonical slugs so completion can
    // never drift from what the parser understands.
    const COMPLETED_KINDS: [ContentKind; 6] = [
        ContentKind::Text,
        ContentKind::Url,
        ContentKind::Image,
        ContentKind::Code,
        ContentKind::File,
        ContentKind::Color,
    ];
    let candidates: Vec<String> = if lower.starts_with("kind:") {
        COMPLETED_KINDS
            .iter()
            .map(|kind| format!("kind:{}", kind.slug()))
            .collect()
    } else {
        ["app:", "kind:", "tag:", "device:", "before:", "after:"]
            .iter()
            .map(|prefix| (*prefix).to_owned())
            .collect()
    };
    candidates
        .into_iter()
        .filter(|candidate| candidate.starts_with(&lower))
        .take(MAX_COMPLETIONS)
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

    fn hits(clips: &[Clip], raw: &str, at: chrono::DateTime<Utc>) -> usize {
        let query = parse_natural_query(raw, at).expect("query should parse");
        search_recall(clips, &query, RecallSearchContext::default()).len()
    }

    /// Characterizes how the parsed `app:` facet is applied: a case-insensitive
    /// *substring* test against `ClipMeta::source_app`, not an exact match.
    #[test]
    fn app_facet_is_a_case_insensitive_substring_match_for_ascii_names() {
        let now = Utc.timestamp_opt(1_000, 0).unwrap();
        let clips = vec![clip("release notes", "Chrome", ContentKind::Url, now)];
        for raw in [
            "app:Chrome",
            "app:chrome",
            "app:CHROME",
            "app:chr",
            "app:ome",
        ] {
            assert_eq!(hits(&clips, raw, now), 1, "app facet {raw}");
        }
        assert_eq!(hits(&clips, "app:firefox", now), 0);
        // A clip without a source app never satisfies the facet.
        let mut anonymous = clips[0].clone();
        anonymous.meta.source_app = None;
        assert_eq!(hits(&[anonymous], "app:chrome", now), 0);
    }

    /// KNOWN BUG, recorded not fixed (theme T1 in
    /// `docs/solid-dry-review-2026-07-26.md`).
    ///
    /// `query.rs::set_once` normalizes the `app:` value with
    /// `str::to_ascii_lowercase`, which leaves every non-ASCII letter as
    /// typed, while `matches_filters` above normalizes the clip side with the
    /// Unicode-aware `str::to_lowercase`. For a non-ASCII application name the
    /// two normalizations disagree, so a user who types the app name exactly
    /// as it was captured gets zero results. Only a pre-lowercased spelling
    /// works. The free-text term does not have this problem because both sides
    /// of that comparison use `to_lowercase`.
    #[test]
    fn app_facet_currently_misses_non_ascii_source_apps_typed_as_captured() {
        let now = Utc.timestamp_opt(1_000, 0).unwrap();
        let clips = vec![clip("Заметка о релизе", "Продукт", ContentKind::Text, now)];
        let exact = parse_natural_query("app:Продукт", now).unwrap();
        assert_eq!(exact.app.as_deref(), Some("Продукт"));
        assert_eq!(hits(&clips, "app:Продукт", now), 0);
        // The only spelling that works today is the already-lowercase one.
        assert_eq!(hits(&clips, "app:продукт", now), 1);
        // Contrast: the free-text path folds case correctly for non-ASCII.
        assert_eq!(hits(&clips, "Заметка", now), 1);
        assert_eq!(hits(&clips, "заметка", now), 1);
    }

    /// The behavior the T1 facet registry is expected to deliver: one shared
    /// normalization for both sides of the `app:` comparison. Ignored because
    /// it fails today, see the characterization test above for why. Unignore
    /// (and delete the "currently misses" assertions) once the registry
    /// normalizes the query value and `source_app` through one function.
    #[test]
    #[ignore = "known T1 bug: app: facet folds case with to_ascii_lowercase, source_app with Unicode to_lowercase"]
    fn app_facet_should_match_a_non_ascii_source_app_typed_as_captured() {
        let now = Utc.timestamp_opt(1_000, 0).unwrap();
        let clips = vec![clip("Заметка о релизе", "Продукт", ContentKind::Text, now)];
        assert_eq!(hits(&clips, "app:Продукт", now), 1);
        assert_eq!(hits(&clips, "app:ПРОДУКТ", now), 1);
        assert_eq!(hits(&clips, "app:продукт", now), 1);
    }

    /// `device:` is the odd facet out: an exact `eq_ignore_ascii_case` against
    /// `lineage.origin_device`, not a substring test like `app:`.
    #[test]
    fn device_facet_is_exact_ascii_case_insensitive_and_never_a_substring() {
        let now = Utc.timestamp_opt(1_000, 0).unwrap();
        let mut laptop = clip("note", "editor", ContentKind::Text, now);
        laptop.meta.lineage.origin_device = Some("MacBook".into());
        let clips = vec![laptop];
        assert_eq!(hits(&clips, "device:MacBook", now), 1);
        assert_eq!(hits(&clips, "device:macbook", now), 1);
        assert_eq!(hits(&clips, "device:MACBOOK", now), 1);
        // Substring spellings that `app:` would accept are rejected here.
        assert_eq!(hits(&clips, "device:mac", now), 0);
        assert_eq!(hits(&clips, "device:book", now), 0);

        // Non-ASCII device names match only when the spelling is identical:
        // both sides fold ASCII case only, so nothing is lost on an exact
        // spelling, but a case difference is not folded.
        let mut cyrillic = clip("note", "editor", ContentKind::Text, now);
        cyrillic.meta.lineage.origin_device = Some("Ноутбук".into());
        let clips = vec![cyrillic];
        assert_eq!(hits(&clips, "device:Ноутбук", now), 1);
        assert_eq!(hits(&clips, "device:ноутбук", now), 0);
    }

    /// `tag:` folds ASCII case on both sides (`set_once` and
    /// `ClipTags::normalize_label` both use `to_ascii_lowercase`), so it is
    /// symmetric where `app:` is not, but it still cannot fold non-ASCII case.
    #[test]
    fn tag_facet_folds_ascii_case_symmetrically_on_both_sides() {
        let now = Utc.timestamp_opt(1_000, 0).unwrap();
        let ascii = clip("note", "editor", ContentKind::Text, now);
        let cyrillic = clip("другая заметка", "editor", ContentKind::Text, now);
        let mut tags = ClipTags::default();
        assert!(tags.add_tag(ascii.id, "Urgent"));
        assert!(tags.add_tag(cyrillic.id, "Работа"));
        let clips = vec![ascii, cyrillic];
        let search = |raw: &str| {
            let query = parse_natural_query(raw, now).expect("query should parse");
            search_recall(
                &clips,
                &query,
                RecallSearchContext {
                    tags: Some(&tags),
                    ..RecallSearchContext::default()
                },
            )
            .len()
        };
        assert_eq!(search("tag:urgent"), 1);
        assert_eq!(search("tag:URGENT"), 1);
        assert_eq!(search("tag:Работа"), 1);
        assert_eq!(search("tag:работа"), 0);
        // Without a tag registry in the context the facet matches nothing.
        let query = parse_natural_query("tag:urgent", now).unwrap();
        assert!(search_recall(&clips, &query, RecallSearchContext::default()).is_empty());
    }

    /// The whole free-text term is matched as ONE substring, so a facet from
    /// the store grammar that this parser swallowed as text (see
    /// `query.rs::tests::foreign_store_grammar_facets_are_swallowed_as_free_text`)
    /// does not just get ignored: it poisons the text match and the query
    /// silently returns nothing.
    #[test]
    fn a_swallowed_foreign_facet_turns_the_text_term_into_a_dead_literal() {
        let now = Utc.timestamp_opt(1_000, 0).unwrap();
        let clips = vec![clip(
            "release notes on example.com",
            "Chrome",
            ContentKind::Url,
            now,
        )];
        assert_eq!(hits(&clips, "release notes", now), 1);
        assert_eq!(hits(&clips, "release notes host:example.com", now), 0);
        assert_eq!(hits(&clips, "host:example.com", now), 0);
    }

    /// Time facets are half-open: `after` is inclusive, `before` exclusive.
    #[test]
    fn time_facets_form_a_half_open_window() {
        let now = Utc.with_ymd_and_hms(2026, 7, 21, 12, 0, 0).unwrap();
        let boundary = Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap();
        let clips = vec![clip("note", "editor", ContentKind::Text, boundary)];
        assert_eq!(hits(&clips, "after:2026-07-20", now), 1);
        assert_eq!(hits(&clips, "before:2026-07-20", now), 0);
        assert_eq!(hits(&clips, "before:2026-07-21", now), 1);
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
