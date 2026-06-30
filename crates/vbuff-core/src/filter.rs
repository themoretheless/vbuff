//! Search and ranking over clips.
//!
//! MVP search is a case-insensitive substring match over each clip's preview
//! text, plus a simple relevance score. Ordering rule: **pinned first, then by
//! score, then by recency** (newest first).

use vbuff_types::Clip;

/// A scored search hit.
#[derive(Clone, Debug)]
pub struct SearchResult<'a> {
    /// The matched clip.
    pub clip: &'a Clip,
    /// Relevance score; higher is better. 0 for an empty query (recency only).
    pub score: i64,
}

/// Filter and rank clips against a query.
///
/// An empty/whitespace query returns all clips ranked by pinned-then-recency.
/// A non-empty query keeps only clips whose searchable text contains the query
/// (case-insensitively) and ranks by match quality.
pub fn search<'a>(clips: &'a [Clip], query: &str) -> Vec<SearchResult<'a>> {
    let q = query.trim().to_lowercase();

    let mut results: Vec<SearchResult<'a>> = if q.is_empty() {
        clips
            .iter()
            .map(|clip| SearchResult { clip, score: 0 })
            .collect()
    } else {
        clips
            .iter()
            .filter_map(|clip| score_clip(clip, &q).map(|score| SearchResult { clip, score }))
            .collect()
    };

    // Sort: pinned first, then higher score. The sort is *stable*, so items
    // with equal rank keep their input order. Callers are expected to pass
    // `clips` already ordered by recency (newest first), as the store does;
    // this lets dedup-bumped clips float to the top without core needing
    // wall-clock knowledge.
    results.sort_by(|a, b| {
        b.clip
            .pinned
            .cmp(&a.clip.pinned)
            .then_with(|| b.score.cmp(&a.score))
    });

    results
}

/// Score a single clip against a lowercased query, or `None` if no match.
fn score_clip(clip: &Clip, q: &str) -> Option<i64> {
    let haystack = searchable_text(clip).to_lowercase();
    let pos = haystack.find(q)?;

    let mut score: i64 = 100;
    // Earlier matches rank higher.
    score -= pos as i64;
    // Prefix / whole-start match bonus.
    if pos == 0 {
        score += 50;
    }
    // Word-boundary match bonus.
    if pos > 0 {
        let prev = haystack.as_bytes()[pos - 1];
        if prev == b' ' || prev == b'\n' || prev == b'\t' {
            score += 20;
        }
    }
    // Shorter haystacks (more focused content) rank slightly higher.
    score -= (haystack.len() / 64) as i64;

    Some(score)
}

/// The text projection used for searching a clip.
fn searchable_text(clip: &Clip) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(t) = clip.primary_text() {
        parts.push(t.to_string());
    }
    if let Some(app) = &clip.meta.source_app {
        parts.push(app.clone());
    }
    parts.push(clip.meta.kind.label().to_string());
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_types::{Clip, ClipId, ClipMeta, ContentKind, Flavor};

    fn text_clip(text: &str, pinned: bool) -> Clip {
        Clip {
            id: ClipId::new(),
            flavors: vec![Flavor::inline("text/plain", text.as_bytes().to_vec())],
            content_hash: [0u8; 32],
            meta: ClipMeta::now(ContentKind::Text, text.len() as u64, None),
            pinned,
            favorite: false,
        }
    }

    #[test]
    fn empty_query_preserves_input_order() {
        // Caller passes clips already in recency order (newest first).
        let clips = vec![
            text_clip("third", false),
            text_clip("second", false),
            text_clip("first", false),
        ];
        let res = search(&clips, "");
        assert_eq!(res.len(), 3);
        assert_eq!(res[0].clip.primary_text(), Some("third"));
        assert_eq!(res[2].clip.primary_text(), Some("first"));
    }

    #[test]
    fn pinned_floats_to_top() {
        // Pinned clip given later in input still floats above the unpinned one.
        let clips = vec![
            text_clip("newest unpinned", false),
            text_clip("old pinned", true),
        ];
        let res = search(&clips, "");
        assert_eq!(res[0].clip.primary_text(), Some("old pinned"));
    }

    #[test]
    fn substring_is_case_insensitive() {
        let clips = vec![text_clip("Hello World", false), text_clip("goodbye", false)];
        let res = search(&clips, "WORLD");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].clip.primary_text(), Some("Hello World"));
    }

    #[test]
    fn prefix_match_outranks_late_match() {
        let clips = vec![
            text_clip("the cat sat", false),
            text_clip("cat first", false),
        ];
        let res = search(&clips, "cat");
        // "cat first" matches at position 0 -> higher score.
        assert_eq!(res[0].clip.primary_text(), Some("cat first"));
    }

    #[test]
    fn no_match_excluded() {
        let clips = vec![text_clip("apple", false)];
        let res = search(&clips, "banana");
        assert!(res.is_empty());
    }
}
