//! Small, deterministic policies behind the popup's adaptive presentation.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use vbuff_types::{Clip, ContentKind};

#[derive(Clone, Default, PartialEq, Eq)]
pub enum HistoryScope {
    #[default]
    All,
    Kind(ContentKind),
    Snippets,
    Source(String),
}

impl std::fmt::Debug for HistoryScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => formatter.write_str("All"),
            Self::Kind(kind) => formatter.debug_tuple("Kind").field(kind).finish(),
            Self::Snippets => formatter.write_str("Snippets"),
            Self::Source(_) => formatter.write_str("Source([redacted])"),
        }
    }
}

impl HistoryScope {
    pub fn matches(&self, clip: &Clip) -> bool {
        match self {
            Self::All => true,
            Self::Kind(kind) => clip.meta.kind == *kind,
            Self::Snippets => clip.pinned || clip.favorite,
            Self::Source(source) => clip.meta.source_app.as_ref() == Some(source),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::All => "All kinds".into(),
            Self::Kind(kind) => kind.label().into(),
            Self::Snippets => "Snippets".into(),
            Self::Source(_) => "Recent app".into(),
        }
    }

    pub fn from_jump_key(character: char) -> Option<Self> {
        match character.to_ascii_lowercase() {
            'u' => Some(Self::Kind(ContentKind::Url)),
            'i' => Some(Self::Kind(ContentKind::Image)),
            'c' => Some(Self::Kind(ContentKind::Code)),
            'f' => Some(Self::Kind(ContentKind::File)),
            'l' => Some(Self::Kind(ContentKind::Color)),
            's' => Some(Self::Snippets),
            _ => None,
        }
    }
}

const RAPID_SCROLL_POINTS_PER_SECOND: f32 = 1_400.0;
const MAX_DELTA_TEXT_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DensityMode {
    #[default]
    Auto,
    Compact,
    Comfortable,
}

impl DensityMode {
    pub fn row_height(self, viewport_height: f32) -> f32 {
        match self {
            Self::Compact => 54.0,
            Self::Comfortable => 68.0,
            Self::Auto if viewport_height < 560.0 => 54.0,
            Self::Auto => 60.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HandedMode {
    #[default]
    Off,
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiPreferences {
    pub density: DensityMode,
    pub reduced_motion: bool,
    pub large_preview: bool,
    pub handed_mode: HandedMode,
    pub motion_inspector: bool,
    pub show_health_digest: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeliveryCapabilities {
    pub automatic_paste: bool,
    pub sensitive_copy: bool,
}

impl DeliveryCapabilities {
    pub const fn action_label(self) -> &'static str {
        if self.automatic_paste {
            "Paste"
        } else {
            "Copy"
        }
    }

    pub const fn allows(self, sensitive: bool) -> bool {
        !sensitive || self.sensitive_copy
    }
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            density: DensityMode::Auto,
            reduced_motion: false,
            large_preview: true,
            handed_mode: HandedMode::Off,
            motion_inspector: false,
            show_health_digest: false,
        }
    }
}

#[derive(Debug)]
pub struct ScrollTuner {
    sampled_at: Instant,
    velocity: f32,
}

impl ScrollTuner {
    pub fn new(now: Instant) -> Self {
        Self {
            sampled_at: now,
            velocity: 0.0,
        }
    }

    pub fn sample(&mut self, delta_points: f32, now: Instant) {
        let elapsed = now
            .saturating_duration_since(self.sampled_at)
            .as_secs_f32()
            .max(1.0 / 240.0);
        let observed = delta_points.abs() / elapsed;
        let decay = if delta_points == 0.0 { 0.72 } else { 0.35 };
        self.velocity = self.velocity * decay + observed * (1.0 - decay);
        self.sampled_at = now;
    }

    pub fn velocity(&self) -> f32 {
        self.velocity
    }

    pub fn rapid(&self) -> bool {
        self.velocity >= RAPID_SCROLL_POINTS_PER_SECOND
    }
}

#[derive(Debug)]
pub struct FocusLossGuard {
    lost_at: Option<Instant>,
    grace: Duration,
}

impl Default for FocusLossGuard {
    fn default() -> Self {
        Self {
            lost_at: None,
            grace: Duration::from_millis(700),
        }
    }
}

impl FocusLossGuard {
    pub fn update(&mut self, focused: bool, now: Instant) -> FocusLossState {
        if focused {
            self.lost_at = None;
            return FocusLossState::Focused;
        }
        let lost_at = *self.lost_at.get_or_insert(now);
        let elapsed = now.saturating_duration_since(lost_at);
        if elapsed >= self.grace {
            FocusLossState::Expired
        } else {
            FocusLossState::Grace {
                remaining: self.grace - elapsed,
                fraction: 1.0 - elapsed.as_secs_f32() / self.grace.as_secs_f32(),
            }
        }
    }

    pub fn reset(&mut self) {
        self.lost_at = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FocusLossState {
    Focused,
    Grace { remaining: Duration, fraction: f32 },
    Expired,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NearDuplicateDelta {
    pub similarity: f32,
    pub added_chars: usize,
    pub removed_chars: usize,
}

impl NearDuplicateDelta {
    pub fn between(left: &str, right: &str) -> Option<Self> {
        if left.len() > MAX_DELTA_TEXT_BYTES || right.len() > MAX_DELTA_TEXT_BYTES {
            return None;
        }
        let left_tokens = normalized_tokens(left);
        let right_tokens = normalized_tokens(right);
        if left_tokens.is_empty() || right_tokens.is_empty() {
            return None;
        }
        let intersection = left_tokens.intersection(&right_tokens).count();
        let union = left_tokens.union(&right_tokens).count();
        let similarity = intersection as f32 / union.max(1) as f32;
        (similarity >= 0.72).then(|| Self {
            similarity,
            added_chars: right.chars().count().saturating_sub(left.chars().count()),
            removed_chars: left.chars().count().saturating_sub(right.chars().count()),
        })
    }
}

fn normalized_tokens(value: &str) -> BTreeSet<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .take(512)
        .map(str::to_lowercase)
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipBadge {
    Verified,
    Lossless,
    Partial,
    Sensitive,
    LocalOnly,
}

impl ClipBadge {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Verified => "Verified",
            Self::Lossless => "Lossless",
            Self::Partial => "Partial",
            Self::Sensitive => "Sensitive",
            Self::LocalOnly => "Local",
        }
    }
}

pub fn clip_badges(clip: &Clip) -> Vec<ClipBadge> {
    let mut badges = Vec::with_capacity(3);
    if clip.flavors.iter().any(|flavor| !flavor.is_realized()) {
        badges.push(ClipBadge::Partial);
    } else if !clip.flavors.is_empty()
        && clip
            .flavors
            .iter()
            .all(|flavor| flavor.integrity_hash.is_some())
    {
        badges.push(ClipBadge::Verified);
    } else {
        badges.push(ClipBadge::Lossless);
    }
    if clip.meta.sensitive {
        badges.push(ClipBadge::Sensitive);
    }
    if !clip.meta.sync_eligible {
        badges.push(ClipBadge::LocalOnly);
    }
    badges
}

pub fn recency_strength(created_at: DateTime<Utc>, now: DateTime<Utc>) -> f32 {
    let age_minutes = now.signed_duration_since(created_at).num_minutes().max(0) as f32;
    (1.0 - age_minutes / (24.0 * 60.0)).clamp(0.0, 1.0)
}

pub fn match_highlight_alpha(score: i64) -> u8 {
    let confidence = ((score + 32) as f32 / 182.0).clamp(0.0, 1.0);
    (42.0 + confidence * 92.0).round() as u8
}

pub fn contextual_search_hint(clips: &[Clip]) -> &'static str {
    if !clips.iter().any(|clip| clip.meta.kind == ContentKind::Url) {
        "Search links..."
    } else if !clips
        .iter()
        .any(|clip| clip.meta.kind == ContentKind::Color)
    {
        "Search colors..."
    } else if !clips.iter().any(|clip| clip.meta.kind == ContentKind::Code) {
        "Search code..."
    } else {
        "Search history..."
    }
}

pub fn contrast_ratio(left: [u8; 3], right: [u8; 3]) -> f32 {
    let left = relative_luminance(left);
    let right = relative_luminance(right);
    (left.max(right) + 0.05) / (left.min(right) + 0.05)
}

fn relative_luminance(rgb: [u8; 3]) -> f32 {
    let channel = |value: u8| {
        let value = f32::from(value) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2])
}

#[derive(Debug)]
pub struct MotionBudget {
    frame_started_at: Instant,
    last_frame_ms: f32,
    dropped_frames: u64,
}

impl MotionBudget {
    pub fn new(now: Instant) -> Self {
        Self {
            frame_started_at: now,
            last_frame_ms: 0.0,
            dropped_frames: 0,
        }
    }

    pub fn begin_frame(&mut self, now: Instant) {
        let elapsed = now.saturating_duration_since(self.frame_started_at);
        self.last_frame_ms = elapsed.as_secs_f32() * 1_000.0;
        if elapsed > Duration::from_millis(34) {
            self.dropped_frames = self.dropped_frames.saturating_add(1);
        }
        self.frame_started_at = now;
    }

    pub fn last_frame_ms(&self) -> f32 {
        self.last_frame_ms
    }

    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MultilingualSample {
    pub language: &'static str,
    pub text: &'static str,
}

pub const MULTILINGUAL_SAMPLES: &[MultilingualSample] = &[
    MultilingualSample {
        language: "Arabic",
        text: "مرحبا بالعالم - 1234",
    },
    MultilingualSample {
        language: "Hebrew",
        text: "שלום עולם - vbuff",
    },
    MultilingualSample {
        language: "Japanese",
        text: "クリップボード履歴を検索",
    },
    MultilingualSample {
        language: "Chinese",
        text: "快速查找剪贴板历史",
    },
    MultilingualSample {
        language: "Korean",
        text: "클립보드 기록 검색",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_tuner_enters_and_leaves_the_cheap_render_path() {
        let start = Instant::now();
        let mut tuner = ScrollTuner::new(start);
        tuner.sample(500.0, start + Duration::from_millis(16));
        assert!(tuner.rapid());
        for index in 1..=12 {
            tuner.sample(0.0, start + Duration::from_millis(16 + index * 16));
        }
        assert!(!tuner.rapid());
    }

    #[test]
    fn focus_loss_has_a_recoverable_window() {
        let start = Instant::now();
        let mut guard = FocusLossGuard::default();
        assert!(matches!(
            guard.update(false, start),
            FocusLossState::Grace { .. }
        ));
        assert_eq!(
            guard.update(false, start + Duration::from_secs(1)),
            FocusLossState::Expired
        );
        assert_eq!(
            guard.update(true, start + Duration::from_secs(2)),
            FocusLossState::Focused
        );
    }

    #[test]
    fn near_duplicate_delta_is_bounded_and_conservative() {
        let delta = NearDuplicateDelta::between(
            "release build passed on linux and windows",
            "release build passed on linux and macos",
        )
        .unwrap();
        assert!(delta.similarity >= 0.72);
        assert!(NearDuplicateDelta::between("alpha beta", "entirely unrelated").is_none());
    }

    #[test]
    fn contrast_self_audit_matches_wcag_reference_values() {
        assert!((contrast_ratio([0, 0, 0], [255, 255, 255]) - 21.0).abs() < 0.01);
        assert!(contrast_ratio([120, 120, 120], [255, 255, 255]) < 4.5);
    }

    #[test]
    fn history_jump_keys_are_bounded_and_source_debug_is_redacted() {
        assert_eq!(
            HistoryScope::from_jump_key('U'),
            Some(HistoryScope::Kind(ContentKind::Url))
        );
        assert_eq!(
            HistoryScope::from_jump_key('s'),
            Some(HistoryScope::Snippets)
        );
        assert_eq!(HistoryScope::from_jump_key('x'), None);
        assert!(!format!("{:?}", HistoryScope::Source("private.app".into())).contains("private"));
    }

    #[test]
    fn delivery_defaults_to_non_sensitive_copy_only() {
        let delivery = DeliveryCapabilities::default();
        assert_eq!(delivery.action_label(), "Copy");
        assert!(!delivery.automatic_paste);
        assert!(delivery.allows(false));
        assert!(!delivery.allows(true));

        let native = DeliveryCapabilities {
            automatic_paste: true,
            sensitive_copy: true,
        };
        assert_eq!(native.action_label(), "Paste");
        assert!(native.allows(true));
    }
}
