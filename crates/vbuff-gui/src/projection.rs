//! Pure history projection for the native popup.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use vbuff_core::recall::{
    MatchExplanation, RecallSearchContext, parse_natural_query, search_recall,
};
use vbuff_types::{Clip, ClipId};

use crate::experience::{HistoryScope, NearDuplicateDelta};

#[derive(Clone, Copy, Debug)]
pub(crate) struct FilteredClip {
    pub id: ClipId,
    pub score: i64,
    pub match_explanation: Option<MatchExplanation>,
    pub duplicate_delta: Option<NearDuplicateDelta>,
    pub hidden_variants: usize,
    pub variant_of: Option<ClipId>,
}

pub(crate) fn filter_clips(
    clips: &[Clip],
    raw_query: &str,
    history_scope: &HistoryScope,
    expanded_duplicates: &HashSet<ClipId>,
    now: DateTime<Utc>,
) -> Vec<FilteredClip> {
    let Ok(query) = parse_natural_query(raw_query, now) else {
        // Invalid structured syntax never falls back to a raw scan: doing so
        // could inspect sensitive payloads under a malformed filter.
        return Vec::new();
    };
    let results = search_recall(clips, &query, RecallSearchContext::default());
    let mut filtered: Vec<FilteredClip> = Vec::with_capacity(results.len());
    let mut root: Option<(ClipId, vbuff_types::ContentKind, String)> = None;
    for result in results
        .into_iter()
        .filter(|result| !clip_is_expired(result.clip, now) && history_scope.matches(result.clip))
    {
        let clip = result.clip;
        let text = (!clip.meta.sensitive)
            .then(|| clip.primary_text())
            .flatten()
            .map(str::to_owned);
        let duplicate = root.as_ref().and_then(|(root_id, kind, root_text)| {
            (clip.meta.kind == *kind)
                .then_some(text.as_deref())
                .flatten()
                .and_then(|text| NearDuplicateDelta::between(text, root_text))
                .map(|delta| (*root_id, delta))
        });
        if let Some((root_id, delta)) = duplicate {
            if expanded_duplicates.contains(&root_id) {
                if let Some(root_hit) = filtered.iter_mut().rev().find(|hit| hit.id == root_id) {
                    root_hit.hidden_variants = root_hit.hidden_variants.saturating_add(1);
                    root_hit.duplicate_delta.get_or_insert(delta);
                }
                filtered.push(FilteredClip {
                    id: clip.id,
                    score: result.score,
                    match_explanation: preferred_match_explanation(&result.explanations),
                    duplicate_delta: Some(delta),
                    hidden_variants: 0,
                    variant_of: Some(root_id),
                });
            } else if let Some(root_hit) = filtered.iter_mut().rev().find(|hit| hit.id == root_id) {
                root_hit.hidden_variants = root_hit.hidden_variants.saturating_add(1);
                root_hit.duplicate_delta.get_or_insert(delta);
            }
            continue;
        }
        filtered.push(FilteredClip {
            id: clip.id,
            score: result.score,
            match_explanation: preferred_match_explanation(&result.explanations),
            duplicate_delta: None,
            hidden_variants: 0,
            variant_of: None,
        });
        root = text.map(|text| (clip.id, clip.meta.kind, text));
    }
    filtered
}

pub(crate) fn clip_is_expired(clip: &Clip, now: DateTime<Utc>) -> bool {
    clip.meta
        .expires_at
        .is_some_and(|expires_at| expires_at <= now)
}

fn preferred_match_explanation(explanations: &[MatchExplanation]) -> Option<MatchExplanation> {
    [
        MatchExplanation::QueryPinned,
        MatchExplanation::PinnedAlias,
        MatchExplanation::TypoCorrection,
        MatchExplanation::Text,
        MatchExplanation::SourceApplication,
        MatchExplanation::Kind,
        MatchExplanation::Tag,
        MatchExplanation::Device,
        MatchExplanation::Time,
        MatchExplanation::DestinationAffinity,
    ]
    .into_iter()
    .find(|candidate| explanations.contains(candidate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_types::{ClipMeta, ContentKind, Flavor};

    fn clip(text: &str, sensitive: bool) -> Clip {
        let flavors = vec![Flavor::inline("text/plain", text.as_bytes().to_vec())];
        let mut meta = ClipMeta::now(ContentKind::Text, text.len() as u64, None);
        meta.sensitive = sensitive;
        Clip {
            id: ClipId::new(),
            content_hash: vbuff_core::content_hash_from_flavors(&flavors),
            flavors,
            meta,
            pinned: false,
            favorite: false,
        }
    }

    #[test]
    fn malformed_structured_query_fails_closed() {
        let clips = [clip("ordinary", false), clip("secret needle", true)];
        assert!(
            filter_clips(
                &clips,
                "unknown:needle",
                &HistoryScope::All,
                &HashSet::new(),
                Utc::now(),
            )
            .is_empty()
        );
    }

    #[test]
    fn expired_sensitive_clip_is_removed_before_render_projection() {
        let now = Utc::now();
        let mut expired = clip("secret needle", true);
        expired.meta.expires_at = Some(now - chrono::Duration::milliseconds(1));

        assert!(filter_clips(&[expired], "", &HistoryScope::All, &HashSet::new(), now,).is_empty());
    }
}
