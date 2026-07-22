//! Search and ranking over clips.
//!
//! MVP search is a case-insensitive substring match over each clip's preview
//! text, plus a simple relevance score. Ordering rule: **pinned first, then by
//! score, then by recency** (newest first).

use vbuff_types::Clip;

use crate::recall::{RecallSearchContext, parse_natural_query, search_recall};

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
    let now = chrono::Utc::now();
    let Ok(parsed) = parse_natural_query(query, now) else {
        // Never reinterpret malformed structured syntax as a raw payload scan.
        return Vec::new();
    };
    search_recall(clips, &parsed, RecallSearchContext::default())
        .into_iter()
        .filter(|result| {
            result
                .clip
                .meta
                .expires_at
                .is_none_or(|expires_at| expires_at > now)
        })
        .map(|result| SearchResult {
            clip: result.clip,
            score: result.score,
        })
        .collect()
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

    #[test]
    fn malformed_query_never_falls_back_to_sensitive_payload_search() {
        let mut sensitive = text_clip("private needle", false);
        sensitive.meta.sensitive = true;
        assert!(search(&[sensitive], "\"needle").is_empty());
    }

    #[test]
    fn expired_clips_are_not_search_results() {
        let mut expired = text_clip("expired", false);
        expired.meta.expires_at = Some(chrono::Utc::now() - chrono::Duration::seconds(1));
        assert!(search(&[expired], "").is_empty());
    }
}
