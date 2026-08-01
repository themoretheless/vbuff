//! Pure history projection for the native popup, plus its memoization.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use vbuff_core::recall::{
    MatchExplanation, NaturalQuery, QueryParseError, RecallSearchContext, parse_natural_query,
    search_recall,
};
use vbuff_types::{Clip, ClipId};

use crate::experience::{HistoryScope, NearDuplicateDelta};

#[derive(Clone, Copy, Debug, PartialEq)]
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

/// Memoized [`filter_clips`], keyed on every input the projection reads.
///
/// The projection is the popup's per-frame hot spot: at the default 500-clip
/// history cap it costs single-digit milliseconds, and the popup repaints on
/// every input event plus a forced refresh each second. Everything else the
/// frame recomputes (`complete_query`, `recent_source_apps`,
/// `contextual_search_hint`, the `clip_by_id` index) measures in fractions of
/// a microsecond and is deliberately left alone.
///
/// The cache never calls the projection's internals: on a miss it calls
/// [`filter_clips`] itself, so a hit and a recompute cannot drift apart.
///
/// # Why time is part of the key
///
/// Four of the five inputs compare trivially. The fifth, `now`, is the reason
/// this is a hand-written type rather than a map:
///
/// * [`parse_natural_query`] resolves relative windows (`today`, `last 2h`)
///   against `now`, so the *parsed* query is the key rather than the raw
///   string. Two raw strings that parse to the same query are cache-equivalent
///   by construction, and a sliding window invalidates the moment it slides.
/// * [`clip_is_expired`] flips at each clip's `expires_at`. Keying on
///   `AppState::revision` alone would leave an expired — possibly sensitive —
///   clip on screen until some unrelated edit invalidated the entry, and the
///   snapshot's own expiry sweep is a background idle job on a 60 s..15 min
///   backoff (`src/maintenance.rs`), not a per-frame guarantee. So the entry
///   carries an explicit horizon: the earliest `expires_at` still in the
///   future. Reaching it kills the entry.
///
/// Those two are the projection's whole time surface. `search_recall`,
/// `HistoryScope::matches` and `NearDuplicateDelta::between` are all pure in
/// the clip list and the query.
#[derive(Default)]
pub(crate) struct ProjectionCache {
    entry: Option<CachedProjection>,
    /// Recompute counter, so tests can prove the cache actually caches.
    #[cfg(test)]
    computations: u64,
}

struct CachedProjection {
    /// Clip-list identity. `AppState::set_clips` is the only path that
    /// replaces the list, and it bumps this; see the test beside it.
    revision: u64,
    /// The parsed query, not the raw string: this is what the projection
    /// actually filters on, and it already folds `now` into relative windows.
    query: Result<NaturalQuery, QueryParseError>,
    scope: HistoryScope,
    expanded_duplicates: HashSet<ClipId>,
    /// Wall clock the entry was computed at. A backwards clock jump can
    /// un-expire a clip, so entries are only reused going forward.
    computed_at: DateTime<Utc>,
    /// Exclusive upper bound on reuse: the earliest `expires_at` that was
    /// still in the future at `computed_at`.
    expiry_horizon: DateTime<Utc>,
    projection: Arc<[FilteredClip]>,
}

impl ProjectionCache {
    /// The projection for this frame, recomputed only when an input changed.
    pub(crate) fn projection(
        &mut self,
        clips: &[Clip],
        revision: u64,
        raw_query: &str,
        history_scope: &HistoryScope,
        expanded_duplicates: &HashSet<ClipId>,
        now: DateTime<Utc>,
    ) -> Arc<[FilteredClip]> {
        // Key material only; `filter_clips` parses again on a miss. The double
        // parse costs well under a microsecond and buys the guarantee that the
        // cached path and the recomputed path run the same code.
        let query = parse_natural_query(raw_query, now);
        if let Some(entry) = &self.entry
            && entry.revision == revision
            && entry.query == query
            && entry.scope == *history_scope
            && entry.expanded_duplicates == *expanded_duplicates
            && entry.computed_at <= now
            && now < entry.expiry_horizon
        {
            return Arc::clone(&entry.projection);
        }

        let projection: Arc<[FilteredClip]> =
            filter_clips(clips, raw_query, history_scope, expanded_duplicates, now).into();
        #[cfg(test)]
        {
            self.computations += 1;
        }
        self.entry = Some(CachedProjection {
            revision,
            query,
            scope: history_scope.clone(),
            expanded_duplicates: expanded_duplicates.clone(),
            computed_at: now,
            expiry_horizon: next_expiry(clips, now),
            projection: Arc::clone(&projection),
        });
        projection
    }

    #[cfg(test)]
    fn computations(&self) -> u64 {
        self.computations
    }
}

/// The earliest moment a clip in this snapshot stops being renderable.
///
/// `DateTime::<Utc>::MAX_UTC` when nothing in the snapshot expires, which is
/// the common case: expiry only applies to clips given a TTL.
fn next_expiry(clips: &[Clip], now: DateTime<Utc>) -> DateTime<Utc> {
    clips
        .iter()
        .filter_map(|clip| clip.meta.expires_at)
        .filter(|expires_at| *expires_at > now)
        .min()
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
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

    // --- ProjectionCache -------------------------------------------------

    const REVISION: u64 = 7;

    fn seconds(count: i64) -> chrono::Duration {
        chrono::Duration::seconds(count)
    }

    fn kinded(text: &str, kind: ContentKind, app: Option<&str>) -> Clip {
        let flavors = vec![Flavor::inline("text/plain", text.as_bytes().to_vec())];
        let meta = ClipMeta::now(kind, text.len() as u64, app.map(str::to_owned));
        Clip {
            id: ClipId::new(),
            content_hash: vbuff_core::content_hash_from_flavors(&flavors),
            flavors,
            meta,
            pinned: false,
            favorite: false,
        }
    }

    /// A deliberately awkward fixture: near-duplicate runs, a pinned clip, a
    /// sensitive clip, mixed kinds and source apps, and two clips with
    /// different TTLs so the expiry horizon has something to pick a minimum
    /// from.
    fn fixture(now: DateTime<Utc>) -> Vec<Clip> {
        let mut clips = vec![
            kinded(
                "release notes for the July build",
                ContentKind::Text,
                Some("Editor"),
            ),
            kinded(
                "release notes for the July build v2",
                ContentKind::Text,
                Some("Editor"),
            ),
            kinded(
                "release notes for the July build v3",
                ContentKind::Text,
                Some("Editor"),
            ),
            kinded(
                "https://example.test/download",
                ContentKind::Url,
                Some("Chrome"),
            ),
            kinded("#ff8800", ContentKind::Color, Some("Figma")),
            kinded(
                "fn main() { println!(\"release\"); }",
                ContentKind::Code,
                Some("Editor"),
            ),
            clip("secret needle", true),
            kinded("unrelated shopping list", ContentKind::Text, Some("Notes")),
        ];
        clips[3].pinned = true;
        // The later deadline first, so a cache that took the first expiry it
        // saw instead of the minimum would keep the shorter-lived clip alive.
        clips[4].meta.expires_at = Some(now + seconds(90));
        clips[6].meta.expires_at = Some(now + seconds(30));
        for clip in &mut clips {
            clip.meta.created_at = now;
        }
        // Parked one second inside a one-hour window, so a sliding relative
        // query ("last 1h") drops it after two seconds of clock. Without a
        // clip on that edge, keying the cache on the raw query string instead
        // of the parsed one would produce identical results and the
        // equivalence matrix would wave the bug through.
        let mut on_the_hour_edge = clip("aging draft", false);
        on_the_hour_edge.meta.created_at = now - seconds(3_599);
        clips.push(on_the_hour_edge);
        clips
    }

    fn expanded_on(clips: &[Clip], index: usize) -> HashSet<ClipId> {
        HashSet::from([clips[index].id])
    }

    /// The privacy invariant. A clip whose TTL lapses between two frames must
    /// leave the projection even though nothing else changed: same clip list,
    /// same revision, same query, same scope. Serving the cached answer here
    /// would keep a sensitive clip on screen for as long as the popup stayed
    /// otherwise idle, and the snapshot's own expiry sweep is a background
    /// idle job on a 60 s..15 min backoff, not a per-frame guarantee.
    #[test]
    fn a_clip_expiring_between_frames_leaves_the_cached_projection() {
        let now = Utc::now();
        let mut secret = clip("secret needle", true);
        secret.meta.expires_at = Some(now + seconds(30));
        let secret_id = secret.id;
        let clips = [clip("ordinary", false), secret];
        let scope = HistoryScope::All;
        let expanded = HashSet::new();
        let mut cache = ProjectionCache::default();

        let before = cache.projection(&clips, REVISION, "", &scope, &expanded, now);
        assert_eq!(before.len(), 2);
        assert!(before.iter().any(|hit| hit.id == secret_id));

        // Exactly at the deadline: `expires_at <= now` is already expired, so
        // the horizon must be exclusive.
        let at_deadline =
            cache.projection(&clips, REVISION, "", &scope, &expanded, now + seconds(30));
        assert_eq!(at_deadline.len(), 1);
        assert!(!at_deadline.iter().any(|hit| hit.id == secret_id));
        assert_eq!(
            cache.computations(),
            2,
            "the TTL lapse must force a recompute"
        );
    }

    /// The horizon is the *earliest* future deadline in the snapshot, not the
    /// first one encountered.
    #[test]
    fn the_expiry_horizon_is_the_earliest_deadline_not_the_first_one() {
        let now = Utc::now();
        let clips = fixture(now);
        let scope = HistoryScope::All;
        let expanded = HashSet::new();
        let mut cache = ProjectionCache::default();

        let first = cache.projection(&clips, REVISION, "", &scope, &expanded, now);
        // Still inside both TTLs: a genuine hit.
        let inside = cache.projection(&clips, REVISION, "", &scope, &expanded, now + seconds(29));
        assert_eq!(&*first, &*inside);
        assert_eq!(cache.computations(), 1);

        // Past the shorter TTL (30 s) but not the longer one (90 s).
        let after_short =
            cache.projection(&clips, REVISION, "", &scope, &expanded, now + seconds(31));
        assert_eq!(cache.computations(), 2);
        assert_eq!(
            &*after_short,
            filter_clips(&clips, "", &scope, &expanded, now + seconds(31)).as_slice()
        );
        assert_eq!(after_short.len(), first.len() - 1);
    }

    /// A cache that changes the answer is worse than no cache. For a matrix of
    /// keys and probe times, the value served — hit or miss — must be byte-for
    /// -byte what a fresh `filter_clips` at that instant produces.
    #[test]
    fn a_cached_projection_is_identical_to_a_fresh_recompute() {
        let now = Utc::now();
        let clips = fixture(now);
        let queries = [
            "",
            "release",
            "releaze",
            "kind:url",
            "kind:code app:editor",
            "urls from chrome",
            "last 1h",
            "today",
            "unknown:needle",
            "\"unterminated",
            "zzzznomatch",
        ];
        let scopes = [
            HistoryScope::All,
            HistoryScope::Kind(ContentKind::Text),
            HistoryScope::Snippets,
            HistoryScope::Source("Editor".to_owned()),
        ];
        let expansions = [
            HashSet::new(),
            expanded_on(&clips, 0),
            expanded_on(&clips, 3),
        ];
        // Probe times straddle both TTLs in the fixture, so the matrix covers
        // hits, expiry-driven misses and clock-driven query drift.
        let probes = [0, 1, 29, 30, 31, 89, 90, 91, 3_600];

        let mut cache = ProjectionCache::default();
        for query in queries {
            for scope in &scopes {
                for expanded in &expansions {
                    for offset in probes {
                        let at = now + seconds(offset);
                        let cached = cache.projection(&clips, REVISION, query, scope, expanded, at);
                        let fresh = filter_clips(&clips, query, scope, expanded, at);
                        assert_eq!(
                            &*cached,
                            fresh.as_slice(),
                            "cache diverged for query {query:?}, scope {scope:?}, +{offset}s"
                        );
                    }
                }
            }
        }
    }

    /// The point of the exercise: an idle frame must not recompute.
    #[test]
    fn repeated_frames_with_the_same_inputs_compute_once() {
        let now = Utc::now();
        let clips = fixture(now);
        let scope = HistoryScope::All;
        let expanded = HashSet::new();
        let mut cache = ProjectionCache::default();

        let first = cache.projection(&clips, REVISION, "release", &scope, &expanded, now);
        for frame in 1..120 {
            let at = now + chrono::Duration::milliseconds(frame * 16);
            let again = cache.projection(&clips, REVISION, "release", &scope, &expanded, at);
            assert_eq!(&*first, &*again);
        }
        assert_eq!(cache.computations(), 1, "120 frames should compute once");
    }

    /// `AppState::revision` is the clip list's identity. A newly captured clip
    /// arrives with a bumped revision and must show up on the same frame, even
    /// though nothing else about the key moved.
    #[test]
    fn a_new_clip_revision_recomputes_against_the_new_list() {
        let now = Utc::now();
        let clips = fixture(now);
        let scope = HistoryScope::All;
        let expanded = HashSet::new();
        let mut cache = ProjectionCache::default();

        let before = cache.projection(&clips, REVISION, "", &scope, &expanded, now);
        cache.projection(&clips, REVISION, "", &scope, &expanded, now);
        assert_eq!(cache.computations(), 1);

        let mut grown = clips.clone();
        let arrival = clip("freshly captured", false);
        let arrival_id = arrival.id;
        grown.insert(0, arrival);
        let after = cache.projection(&grown, REVISION + 1, "", &scope, &expanded, now);
        assert_eq!(cache.computations(), 2);
        assert_eq!(after.len(), before.len() + 1);
        assert!(after.iter().any(|hit| hit.id == arrival_id));
        assert_eq!(
            &*after,
            filter_clips(&grown, "", &scope, &expanded, now).as_slice()
        );
    }

    #[test]
    fn query_scope_and_expanded_duplicates_are_all_part_of_the_key() {
        let now = Utc::now();
        let clips = fixture(now);
        let all = HistoryScope::All;
        let empty = HashSet::new();
        let mut cache = ProjectionCache::default();

        cache.projection(&clips, REVISION, "", &all, &empty, now);
        assert_eq!(cache.computations(), 1);
        cache.projection(&clips, REVISION, "release", &all, &empty, now);
        assert_eq!(cache.computations(), 2);
        cache.projection(
            &clips,
            REVISION,
            "release",
            &HistoryScope::Kind(ContentKind::Text),
            &empty,
            now,
        );
        assert_eq!(cache.computations(), 3);
        cache.projection(
            &clips,
            REVISION,
            "release",
            &HistoryScope::Kind(ContentKind::Text),
            &expanded_on(&clips, 0),
            now,
        );
        assert_eq!(cache.computations(), 4);
    }

    /// The key is the *parsed* query, so two spellings of the same query share
    /// one entry — and, more importantly, a query whose meaning depends on the
    /// clock cannot silently reuse an entry parsed against a different one.
    #[test]
    fn the_key_is_the_parsed_query_not_the_raw_string() {
        let now = Utc::now();
        let clips = fixture(now);
        let scope = HistoryScope::All;
        let expanded = HashSet::new();
        let mut cache = ProjectionCache::default();

        cache.projection(&clips, REVISION, "urls from chrome", &scope, &expanded, now);
        cache.projection(
            &clips,
            REVISION,
            "kind:url app:chrome",
            &scope,
            &expanded,
            now,
        );
        assert_eq!(cache.computations(), 1, "equivalent spellings are one key");
    }

    #[test]
    fn a_sliding_relative_window_invalidates_as_it_slides() {
        let now = Utc::now();
        let clips = fixture(now);
        let scope = HistoryScope::All;
        let expanded = HashSet::new();
        let mut cache = ProjectionCache::default();

        cache.projection(&clips, REVISION, "last 1h", &scope, &expanded, now);
        cache.projection(&clips, REVISION, "last 1h", &scope, &expanded, now);
        assert_eq!(cache.computations(), 1, "the same instant is still a hit");
        cache.projection(
            &clips,
            REVISION,
            "last 1h",
            &scope,
            &expanded,
            now + seconds(1),
        );
        assert_eq!(
            cache.computations(),
            2,
            "a moved window is a different query"
        );

        // A day-anchored window, by contrast, is stable until UTC midnight.
        cache.projection(&clips, REVISION, "today", &scope, &expanded, now);
        let before = cache.computations();
        cache.projection(
            &clips,
            REVISION,
            "today",
            &scope,
            &expanded,
            now + seconds(1),
        );
        assert_eq!(cache.computations(), before, "day anchors do not slide");
    }

    /// A clock that steps backwards can un-expire a clip, so entries are never
    /// reused for an earlier instant than the one they were computed at.
    #[test]
    fn a_backwards_clock_step_recomputes() {
        let now = Utc::now();
        let clips = fixture(now);
        let scope = HistoryScope::All;
        let expanded = HashSet::new();
        let mut cache = ProjectionCache::default();

        cache.projection(&clips, REVISION, "", &scope, &expanded, now);
        cache.projection(&clips, REVISION, "", &scope, &expanded, now - seconds(1));
        assert_eq!(cache.computations(), 2);
    }

    /// A snapshot with no TTLs anywhere must not pin an accidental horizon.
    #[test]
    fn a_snapshot_without_deadlines_has_an_unbounded_horizon() {
        let now = Utc::now();
        let clips = [clip("ordinary", false), clip("another", false)];
        assert_eq!(next_expiry(&clips, now), DateTime::<Utc>::MAX_UTC);

        let scope = HistoryScope::All;
        let expanded = HashSet::new();
        let mut cache = ProjectionCache::default();
        cache.projection(&clips, REVISION, "", &scope, &expanded, now);
        cache.projection(
            &clips,
            REVISION,
            "",
            &scope,
            &expanded,
            now + seconds(86_400),
        );
        assert_eq!(cache.computations(), 1);
    }

    /// Already-expired clips must not drag the horizon into the past, which
    /// would make every frame a miss.
    #[test]
    fn an_already_expired_clip_does_not_pull_the_horizon_backwards() {
        let now = Utc::now();
        let mut stale = clip("gone", false);
        stale.meta.expires_at = Some(now - seconds(1));
        let mut live = clip("still here", false);
        live.meta.expires_at = Some(now + seconds(60));
        let clips = [stale, live];

        assert_eq!(next_expiry(&clips, now), now + seconds(60));
    }
}
