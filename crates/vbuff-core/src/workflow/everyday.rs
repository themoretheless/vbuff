//! Small, content-conscious policies for everyday history maintenance.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;
use std::time::Duration;

use chrono::{DateTime, Datelike as _, Utc};
use thiserror::Error;
use url::Url;
use vbuff_types::{CaptureLineage, Clip, ClipId, ContentKind, Flavor};

use crate::content_hash_from_flavors;

const MAX_BEHAVIOR_APP_BYTES: usize = 512;
const MAX_BURST_INPUTS: usize = 10_000;
const MAX_OBSERVATION_KEYS: usize = 1_024;
const MAX_OBSERVATION_URL_BYTES: usize = 16 * 1024;
const MAX_SESSION_PROTECTED: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BehaviorAction {
    SkipCapture,
    PlainTextPaste,
    CleanLink,
}

#[derive(Clone, PartialEq, Eq)]
pub struct RuleSuggestion {
    source_app: String,
    action: BehaviorAction,
    observations: u16,
}

impl fmt::Debug for RuleSuggestion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuleSuggestion")
            .field("source_app", &"[redacted]")
            .field("action", &self.action)
            .field("observations", &self.observations)
            .finish()
    }
}

impl RuleSuggestion {
    pub fn source_app(&self) -> &str {
        &self.source_app
    }

    pub const fn action(&self) -> BehaviorAction {
        self.action
    }

    pub const fn observations(&self) -> u16 {
        self.observations
    }
}

#[derive(Clone)]
pub struct RuleSuggestionEngine {
    threshold: u16,
    observations: BTreeMap<(String, BehaviorAction), u16>,
    offered: BTreeSet<(String, BehaviorAction)>,
    dismissed: BTreeSet<(String, BehaviorAction)>,
}

impl fmt::Debug for RuleSuggestionEngine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuleSuggestionEngine")
            .field("threshold", &self.threshold)
            .field("observation_keys", &self.observations.len())
            .field("offered", &self.offered.len())
            .field("dismissed", &self.dismissed.len())
            .finish()
    }
}

impl RuleSuggestionEngine {
    pub fn new(threshold: u16) -> Self {
        Self {
            threshold: threshold.max(2),
            observations: BTreeMap::new(),
            offered: BTreeSet::new(),
            dismissed: BTreeSet::new(),
        }
    }

    pub fn observe(&mut self, source_app: &str, action: BehaviorAction) -> Option<RuleSuggestion> {
        if !valid_app(source_app) {
            return None;
        }
        let key = (source_app.to_owned(), action);
        if !self.observations.contains_key(&key) && self.observations.len() >= MAX_OBSERVATION_KEYS
        {
            return None;
        }
        let count = self.observations.entry(key.clone()).or_default();
        *count = count.saturating_add(1);
        if *count < self.threshold || self.offered.contains(&key) || self.dismissed.contains(&key) {
            return None;
        }
        self.offered.insert(key);
        Some(RuleSuggestion {
            source_app: source_app.to_owned(),
            action,
            observations: *count,
        })
    }

    pub fn dismiss(&mut self, suggestion: &RuleSuggestion) -> bool {
        let key = (suggestion.source_app.clone(), suggestion.action);
        if !valid_app(&suggestion.source_app) || !self.offered.remove(&key) {
            return false;
        }
        self.dismissed.insert(key)
    }
}

impl Default for RuleSuggestionEngine {
    fn default() -> Self {
        Self::new(3)
    }
}

fn valid_app(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_BEHAVIOR_APP_BYTES
        && !value.chars().any(char::is_control)
}

#[derive(Clone, PartialEq, Eq)]
pub struct CopyBurst {
    pub clip_ids: Vec<ClipId>,
    pub source_app: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
}

impl fmt::Debug for CopyBurst {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CopyBurst")
            .field("clip_count", &self.clip_ids.len())
            .field("has_source_app", &self.source_app.is_some())
            .field("started_at", &self.started_at)
            .field("ended_at", &self.ended_at)
            .finish()
    }
}

pub fn group_copy_bursts(clips: &[Clip], maximum_gap: Duration) -> Vec<CopyBurst> {
    if clips.len() < 2 || clips.len() > MAX_BURST_INPUTS || maximum_gap.is_zero() {
        return Vec::new();
    }
    let max_gap = chrono::Duration::from_std(maximum_gap).unwrap_or(chrono::Duration::MAX);
    let mut ordered = clips.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|clip| clip.meta.created_at);
    let mut bursts = Vec::new();
    let mut current = vec![ordered[0]];
    let mut current_hashes = HashSet::from([ordered[0].content_hash]);
    for clip in ordered.into_iter().skip(1) {
        let previous = current.last().copied().expect("burst is never empty");
        let same_source =
            clip.meta.source_app.is_some() && clip.meta.source_app == previous.meta.source_app;
        let close = clip.meta.created_at - previous.meta.created_at <= max_gap;
        let distinct = !current_hashes.contains(&clip.content_hash);
        if same_source && close && distinct {
            current.push(clip);
            current_hashes.insert(clip.content_hash);
        } else {
            push_burst(&mut bursts, &current);
            current.clear();
            current_hashes.clear();
            current.push(clip);
            current_hashes.insert(clip.content_hash);
        }
    }
    push_burst(&mut bursts, &current);
    bursts
}

fn push_burst(output: &mut Vec<CopyBurst>, clips: &[&Clip]) {
    if clips.len() < 2 {
        return;
    }
    output.push(CopyBurst {
        clip_ids: clips.iter().map(|clip| clip.id).collect(),
        source_app: clips[0].meta.source_app.clone(),
        started_at: clips[0].meta.created_at,
        ended_at: clips[clips.len() - 1].meta.created_at,
    });
}

#[derive(Clone, Debug, Default)]
pub struct SessionProtection {
    protected: HashSet<ClipId>,
}

impl SessionProtection {
    pub fn set(&mut self, id: ClipId, protected: bool) {
        if protected {
            if self.protected.len() < MAX_SESSION_PROTECTED || self.protected.contains(&id) {
                self.protected.insert(id);
            }
        } else {
            self.protected.remove(&id);
        }
    }

    pub fn contains(&self, id: ClipId) -> bool {
        self.protected.contains(&id)
    }

    pub fn ids(&self) -> impl Iterator<Item = ClipId> + '_ {
        self.protected.iter().copied()
    }
}

pub fn expiry_label(clip: &Clip, now: DateTime<Utc>, retention_days: Option<u32>) -> String {
    if let Some(expires_at) = clip.meta.expires_at {
        if expires_at <= now {
            return "expired".into();
        }
        if expires_at.year() == now.year() && expires_at.ordinal() == now.ordinal() {
            return "expires tonight".into();
        }
        let days = expires_at
            .signed_duration_since(now)
            .num_days()
            .saturating_add(1)
            .max(1);
        return format!("expires in {days}d");
    }
    retention_days.map_or_else(|| "permanent".into(), |days| format!("kept {days}d"))
}

pub fn plain_text_clone(source: &Clip, now: DateTime<Utc>) -> Option<Clip> {
    let text = source
        .flavors
        .iter()
        .find(|flavor| flavor.is_plain_text())
        .or_else(|| source.flavors.iter().find(|flavor| flavor.is_text()))?
        .as_text()?;
    let flavors = vec![Flavor::derived(
        "text/plain;charset=utf-8",
        text.as_bytes().to_vec(),
    )];
    let mut meta = source.meta.clone();
    meta.created_at = now;
    meta.byte_size = text.len() as u64;
    meta.kind = match source.meta.kind {
        ContentKind::Url | ContentKind::Code | ContentKind::Color => source.meta.kind,
        _ => ContentKind::Text,
    };
    meta.generation = None;
    meta.lineage = CaptureLineage::default();
    Some(Clip {
        id: ClipId::new(),
        content_hash: content_hash_from_flavors(&flavors),
        flavors,
        meta,
        pinned: false,
        favorite: false,
    })
}

#[derive(Clone, PartialEq, Eq)]
pub struct DomainRuleSuggestion {
    domain: String,
    observations: u16,
}

impl fmt::Debug for DomainRuleSuggestion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DomainRuleSuggestion")
            .field("domain", &"[redacted]")
            .field("observations", &self.observations)
            .finish()
    }
}

impl DomainRuleSuggestion {
    pub fn domain(&self) -> &str {
        &self.domain
    }

    pub const fn observations(&self) -> u16 {
        self.observations
    }
}

#[derive(Clone)]
pub struct CleanLinkMemory {
    threshold: u16,
    counts: BTreeMap<String, u16>,
    offered: BTreeSet<String>,
    dismissed: BTreeSet<String>,
}

impl fmt::Debug for CleanLinkMemory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CleanLinkMemory")
            .field("threshold", &self.threshold)
            .field("domains", &self.counts.len())
            .field("offered", &self.offered.len())
            .field("dismissed", &self.dismissed.len())
            .finish()
    }
}

impl CleanLinkMemory {
    pub fn new(threshold: u16) -> Self {
        Self {
            threshold: threshold.max(2),
            counts: BTreeMap::new(),
            offered: BTreeSet::new(),
            dismissed: BTreeSet::new(),
        }
    }

    pub fn observe(&mut self, original: &str, cleaned: &str) -> Option<DomainRuleSuggestion> {
        if original == cleaned
            || original.len() > MAX_OBSERVATION_URL_BYTES
            || cleaned.len() > MAX_OBSERVATION_URL_BYTES
        {
            return None;
        }
        let domain = Url::parse(original).ok()?.host_str()?.to_ascii_lowercase();
        if !self.counts.contains_key(&domain) && self.counts.len() >= MAX_OBSERVATION_KEYS {
            return None;
        }
        let count = self.counts.entry(domain.clone()).or_default();
        *count = count.saturating_add(1);
        if *count < self.threshold
            || self.offered.contains(&domain)
            || self.dismissed.contains(&domain)
        {
            return None;
        }
        self.offered.insert(domain.clone());
        Some(DomainRuleSuggestion {
            domain,
            observations: *count,
        })
    }

    pub fn dismiss(&mut self, suggestion: &DomainRuleSuggestion) -> bool {
        if !self.offered.remove(&suggestion.domain) {
            return false;
        }
        self.dismissed.insert(suggestion.domain.clone())
    }
}

impl Default for CleanLinkMemory {
    fn default() -> Self {
        Self::new(3)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeBudgetDecision {
    KeepFull,
    PreviewOnly { maximum_preview_bytes: usize },
    Reject,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum SizeBudgetError {
    #[error("clipboard size budget is invalid")]
    InvalidBudget,
}

impl SizeBudgetDecision {
    pub fn evaluate(
        payload_bytes: usize,
        soft_limit_bytes: usize,
        hard_limit_bytes: usize,
        preview_bytes: usize,
    ) -> Result<Self, SizeBudgetError> {
        if preview_bytes == 0
            || preview_bytes > soft_limit_bytes
            || soft_limit_bytes > hard_limit_bytes
        {
            return Err(SizeBudgetError::InvalidBudget);
        }
        Ok(if payload_bytes > hard_limit_bytes {
            Self::Reject
        } else if payload_bytes > soft_limit_bytes {
            Self::PreviewOnly {
                maximum_preview_bytes: preview_bytes,
            }
        } else {
            Self::KeepFull
        })
    }
}

pub fn recent_source_apps(clips: &[Clip], limit: usize) -> Vec<String> {
    let mut recent = clips.iter().take(MAX_BURST_INPUTS).collect::<Vec<_>>();
    recent.sort_by(|left, right| {
        right
            .meta
            .created_at
            .cmp(&left.meta.created_at)
            .then_with(|| left.meta.source_app.cmp(&right.meta.source_app))
    });
    let mut seen = BTreeSet::new();
    recent
        .into_iter()
        .filter_map(|clip| clip.meta.source_app.as_ref())
        .filter(|app| valid_app(app) && seen.insert((*app).clone()))
        .take(limit.min(16))
        .cloned()
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinReviewCandidate {
    pub clip_id: ClipId,
    pub age_days: u64,
    pub byte_size: u64,
}

pub fn stale_pin_candidates(
    clips: &[Clip],
    now: DateTime<Utc>,
    minimum_age: Duration,
    limit: usize,
) -> Vec<PinReviewCandidate> {
    let minimum_age = chrono::Duration::from_std(minimum_age).unwrap_or(chrono::Duration::MAX);
    clips
        .iter()
        .filter(|clip| clip.pinned && now - clip.meta.created_at >= minimum_age)
        .take(limit.min(100))
        .map(|clip| PinReviewCandidate {
            clip_id: clip.id,
            age_days: (now - clip.meta.created_at).num_days().max(0) as u64,
            byte_size: clip.meta.byte_size,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;
    use vbuff_types::{ClipMeta, Flavor};

    use super::*;

    fn clip(text: &str, app: &str, at: DateTime<Utc>) -> Clip {
        let flavors = vec![Flavor::inline("text/plain", text.as_bytes().to_vec())];
        Clip {
            id: ClipId::new(),
            content_hash: content_hash_from_flavors(&flavors),
            flavors,
            meta: ClipMeta {
                created_at: at,
                ..ClipMeta::now(ContentKind::Text, text.len() as u64, Some(app.into()))
            },
            pinned: false,
            favorite: false,
        }
    }

    #[test]
    fn behavior_suggestions_require_repetition_and_remember_dismissal() {
        let mut engine = RuleSuggestionEngine::new(3);
        assert!(
            engine
                .observe("editor", BehaviorAction::PlainTextPaste)
                .is_none()
        );
        assert!(
            engine
                .observe("editor", BehaviorAction::PlainTextPaste)
                .is_none()
        );
        let suggestion = engine
            .observe("editor", BehaviorAction::PlainTextPaste)
            .unwrap();
        assert_eq!(suggestion.source_app(), "editor");
        assert_eq!(suggestion.action(), BehaviorAction::PlainTextPaste);
        assert_eq!(suggestion.observations(), 3);
        assert!(!format!("{suggestion:?}").contains("editor"));
        assert!(!format!("{engine:?}").contains("editor"));
        assert!(engine.dismiss(&suggestion));
        assert!(!engine.dismiss(&suggestion));
        assert!(
            engine
                .observe("editor", BehaviorAction::PlainTextPaste)
                .is_none()
        );
    }

    #[test]
    fn rapid_distinct_copies_group_without_exposing_source_in_debug() {
        let start = Utc.timestamp_opt(1_000, 0).unwrap();
        let clips = vec![
            clip("a", "sheet", start),
            clip("b", "sheet", start + chrono::Duration::milliseconds(500)),
            clip("c", "browser", start + chrono::Duration::milliseconds(700)),
        ];
        let bursts = group_copy_bursts(&clips, Duration::from_secs(1));
        assert_eq!(bursts.len(), 1);
        assert_eq!(bursts[0].clip_ids.len(), 2);
        assert!(!format!("{:?}", bursts[0]).contains("sheet"));

        let mut unknown_a = clip("d", "ignored", start);
        let mut unknown_b = clip("e", "ignored", start + chrono::Duration::milliseconds(10));
        unknown_a.meta.source_app = None;
        unknown_b.meta.source_app = None;
        assert!(group_copy_bursts(&[unknown_a, unknown_b], Duration::from_secs(1)).is_empty());
    }

    #[test]
    fn plain_clone_preserves_original_and_sensitive_policy() {
        let now = Utc.timestamp_opt(2_000, 0).unwrap();
        let mut original = clip("hello", "editor", now - chrono::Duration::seconds(5));
        original
            .flavors
            .push(Flavor::inline("text/html", b"<b>hello</b>".to_vec()));
        original.meta.sensitive = true;
        original.meta.sync_eligible = false;
        let clone = plain_text_clone(&original, now).unwrap();
        assert_eq!(original.flavors.len(), 2);
        assert_eq!(clone.flavors.len(), 1);
        assert!(clone.flavors[0].is_plain_text());
        assert!(clone.meta.sensitive);
        assert!(!clone.meta.sync_eligible);
        assert_ne!(clone.id, original.id);
    }

    #[test]
    fn expiry_labels_and_size_budget_are_predictable() {
        let now = Utc.with_ymd_and_hms(2026, 7, 20, 9, 0, 0).unwrap();
        let mut expiring = clip("a", "editor", now);
        expiring.meta.expires_at = Some(now + chrono::Duration::hours(2));
        assert_eq!(expiry_label(&expiring, now, None), "expires tonight");
        expiring.meta.expires_at = None;
        assert_eq!(expiry_label(&expiring, now, Some(7)), "kept 7d");
        assert_eq!(
            SizeBudgetDecision::evaluate(11, 10, 20, 5).unwrap(),
            SizeBudgetDecision::PreviewOnly {
                maximum_preview_bytes: 5
            }
        );
        assert_eq!(
            SizeBudgetDecision::evaluate(21, 10, 20, 5).unwrap(),
            SizeBudgetDecision::Reject
        );
    }

    #[test]
    fn clean_link_memory_and_maintenance_lists_are_bounded() {
        let mut memory = CleanLinkMemory::new(2);
        let original = "https://example.test/?utm_source=a";
        let cleaned = "https://example.test/";
        assert!(memory.observe(original, cleaned).is_none());
        let suggestion = memory.observe(original, cleaned).unwrap();
        assert_eq!(suggestion.domain(), "example.test");
        assert_eq!(suggestion.observations(), 2);
        assert!(!format!("{suggestion:?}").contains("example.test"));
        assert!(!format!("{memory:?}").contains("example.test"));

        let now = Utc.timestamp_opt(10_000_000, 0).unwrap();
        let mut old = clip("old", "editor", now - chrono::Duration::days(100));
        old.pinned = true;
        let recent = clip("new", "browser", now);
        assert_eq!(
            recent_source_apps(&[old.clone(), recent], 1),
            vec!["browser"]
        );
        let alpha = clip("alpha", "alpha", now);
        let zeta = clip("zeta", "zeta", now);
        assert_eq!(recent_source_apps(&[zeta, alpha], 2), vec!["alpha", "zeta"]);
        assert_eq!(
            stale_pin_candidates(&[old], now, Duration::from_secs(90 * 86_400), 10).len(),
            1
        );
    }

    #[test]
    fn session_protection_is_ephemeral_set_membership() {
        let id = ClipId::new();
        let mut protection = SessionProtection::default();
        protection.set(id, true);
        assert!(protection.contains(id));
        assert_eq!(protection.ids().collect::<Vec<_>>(), vec![id]);
        protection.set(id, false);
        assert!(!protection.contains(id));
    }
}
