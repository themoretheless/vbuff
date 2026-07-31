//! Plain data model for vbuff.
//!
//! This crate holds the serializable data types shared by every other crate
//! (core logic, storage, GUI, platform). It deliberately avoids heavy
//! dependencies so it can be linked everywhere cheaply. The one exception is
//! [`mac`], which owns the workspace's single HMAC primitive: it has to sit
//! below every crate that authenticates something, and its `hmac`/`sha2`
//! dependencies are already linked by all of them through `vbuff-core`.
#![forbid(unsafe_code)]
//!
//! The central type is [`Clip`]: one logical copy event that may carry several
//! [`Flavor`]s (one per MIME representation). A clip is identified by a
//! ULID-based [`ClipId`] and deduplicated by a BLAKE3 `content_hash` computed
//! over its canonical flavor bytes (see `vbuff-core`).

mod ipc;
pub mod mac;
pub mod replay;
mod rgba;
mod status;
pub mod validation;

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

pub use ipc::{ClientIntent, ServerResponse};
pub use rgba::{
    RGBA_MIME_PREFIX, is_rgba_mime, parse_rgba_dims, parse_rgba_dims_checked, rgba_mime,
    rgba_required_len,
};
pub use status::{
    CapabilityView, CapabilityViewLevel, CapabilityViewSeverity, CaptureBudgetAlert, CaptureHealth,
    CapturePauseReason, CaptureSessionStats, ClipboardHealthDigest, CommandNotice, NoticeLevel,
    PrivacyDecisionLevel, PrivacyEventSummary, PrivacyLedgerSummary, SecurityPostureLevel,
    SecurityPostureSummary, SelectionSource, SloMetricState, SloStatusSummary,
};

/// A ULID-based identifier for a clip.
///
/// ULIDs are lexicographically sortable by creation time and are friendly to
/// future sync (no central coordinator needed to allocate ids).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClipId(pub Ulid);

impl ClipId {
    /// Generate a fresh id from the current time.
    pub fn new() -> Self {
        ClipId(Ulid::new())
    }

    /// Render as the canonical 26-character Crockford base32 string.
    pub fn to_string_repr(&self) -> String {
        self.0.to_string()
    }

    /// Parse from the canonical 26-character string representation.
    pub fn parse(s: &str) -> Result<Self, ulid::DecodeError> {
        Ok(ClipId(Ulid::from_string(s)?))
    }

    /// The creation timestamp embedded in the ULID, as a UTC datetime.
    pub fn timestamp(&self) -> DateTime<Utc> {
        let ms = self.0.timestamp_ms();
        DateTime::from_timestamp_millis(ms as i64).unwrap_or_else(Utc::now)
    }
}

impl Default for ClipId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ClipId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for ClipId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ClipId({})", self.0)
    }
}

/// The detected primary content kind of a clip, used for icons and filtering.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum ContentKind {
    /// Plain unstructured text.
    Text,
    /// A URL (http/https/ftp/mailto, ...).
    Url,
    /// A color value, e.g. a hex code like `#ff8800`.
    Color,
    /// Source code or a code-like snippet.
    Code,
    /// A raster image.
    Image,
    /// A file or list of files (uri-list / CF_HDROP / NSFilenames).
    File,
    /// Rich Text Format.
    Rtf,
    /// HTML markup.
    Html,
    /// Anything not otherwise classified.
    #[default]
    Other,
}

impl ContentKind {
    /// A short emoji/badge suitable for a compact list row.
    pub fn icon(&self) -> &'static str {
        match self {
            ContentKind::Text => "📄",
            ContentKind::Url => "🔗",
            ContentKind::Color => "🎨",
            ContentKind::Code => "💻",
            ContentKind::Image => "🖼",
            ContentKind::File => "📁",
            ContentKind::Rtf => "📝",
            ContentKind::Html => "🌐",
            ContentKind::Other => "📋",
        }
    }

    /// A short human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            ContentKind::Text => "Text",
            ContentKind::Url => "URL",
            ContentKind::Color => "Color",
            ContentKind::Code => "Code",
            ContentKind::Image => "Image",
            ContentKind::File => "File",
            ContentKind::Rtf => "RTF",
            ContentKind::Html => "HTML",
            ContentKind::Other => "Other",
        }
    }

    /// Every kind, in stored-discriminant order.
    ///
    /// This is the single authoritative enumeration of all kinds; iterate it
    /// instead of hand-writing variant lists. Adding a variant fails
    /// compilation in [`Self::stored_discriminant`] and [`Self::slug`]
    /// (exhaustive matches, no wildcard), and the `content_kind_all_covers_every_variant`
    /// test guard then forces this list to be extended.
    pub const ALL: &'static [ContentKind] = &[
        ContentKind::Text,
        ContentKind::Rtf,
        ContentKind::Html,
        ContentKind::Image,
        ContentKind::File,
        ContentKind::Color,
        ContentKind::Url,
        ContentKind::Code,
        ContentKind::Other,
    ];

    /// The stable integer persisted for this kind (SQLite `clips.kind` and
    /// every other stored surface that records a content kind numerically).
    ///
    /// # Stability contract
    ///
    /// These numbers are an on-disk database format, not an implementation
    /// detail:
    ///
    /// - never change the number of an existing variant;
    /// - never reuse a number after removing a variant;
    /// - a new variant takes the next unused number.
    ///
    /// The mapping intentionally differs from the Rust declaration order: it
    /// preserves the order in which kinds were first persisted. SQL that
    /// hard-codes a kind (for example `kind = 7` for [`ContentKind::Code`] in
    /// the snippet FTS triggers) relies on these exact values.
    pub const fn stored_discriminant(self) -> i64 {
        match self {
            ContentKind::Text => 0,
            ContentKind::Rtf => 1,
            ContentKind::Html => 2,
            ContentKind::Image => 3,
            ContentKind::File => 4,
            ContentKind::Color => 5,
            ContentKind::Url => 6,
            ContentKind::Code => 7,
            ContentKind::Other => 8,
        }
    }

    /// Decode a discriminant read back from storage.
    ///
    /// Fail-closed: unknown values return `None` instead of degrading to
    /// [`ContentKind::Other`]. `Other` is a real stored value (8), not a
    /// fallback; silently mapping unknown numbers onto it would mask database
    /// corruption. Callers decide how to surface the failure (skip the row,
    /// report corruption, and so on).
    ///
    /// Implemented as a scan over [`Self::ALL`] so there is no numeric
    /// wildcard that could silently swallow a newly added variant.
    pub fn from_stored_discriminant(value: i64) -> Option<Self> {
        ContentKind::ALL
            .iter()
            .copied()
            .find(|kind| kind.stored_discriminant() == value)
    }

    /// The stable lowercase ASCII slug for this kind, used in on-disk paths
    /// (content-addressed storage shard directories) and policy files.
    ///
    /// Same stability contract as [`Self::stored_discriminant`]: slugs are a
    /// persisted format; never rename one.
    pub const fn slug(self) -> &'static str {
        match self {
            ContentKind::Text => "text",
            ContentKind::Url => "url",
            ContentKind::Color => "color",
            ContentKind::Code => "code",
            ContentKind::Image => "image",
            ContentKind::File => "file",
            ContentKind::Rtf => "rtf",
            ContentKind::Html => "html",
            ContentKind::Other => "other",
        }
    }
}

/// Error returned when parsing an unrecognized [`ContentKind`] slug.
///
/// Deliberately carries no copy of the rejected input so it can never leak
/// clipboard-adjacent data through logs or error chains.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParseContentKindError;

impl fmt::Display for ParseContentKindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unknown content kind slug")
    }
}

impl std::error::Error for ParseContentKindError {}

impl std::str::FromStr for ContentKind {
    type Err = ParseContentKindError;

    /// Parses the canonical [`slug`](ContentKind::slug) form, exact match
    /// (lowercase, no surrounding whitespace). Callers that accept user input
    /// should normalize case themselves before parsing.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ContentKind::ALL
            .iter()
            .copied()
            .find(|kind| kind.slug() == s)
            .ok_or(ParseContentKindError)
    }
}

/// Non-secret explanation for why a clip is masked and restricted.
///
/// The enum deliberately carries no matched bytes or detector details that
/// could reveal the clipboard payload through metadata, logs, or exports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitivityReason {
    SourceApplication,
    CaptureRule,
    Entropy,
    OperatingSystemHint,
    PrivateKey,
    CloudCredential,
    AccessToken,
    JsonWebToken,
    PaymentCard,
    OneTimePassword,
    RecoveryCode,
    PossibleSecret,
}

impl SensitivityReason {
    /// Payload-free label suitable for a masked preview.
    pub const fn watermark(self) -> &'static str {
        match self {
            Self::SourceApplication => "Masked: source policy",
            Self::CaptureRule => "Masked: capture rule",
            Self::Entropy => "Masked: high entropy",
            Self::OperatingSystemHint => "Masked: system privacy hint",
            Self::PrivateKey => "Masked: private key",
            Self::CloudCredential => "Masked: cloud credential",
            Self::AccessToken => "Masked: access token",
            Self::JsonWebToken => "Masked: signed token",
            Self::PaymentCard => "Masked: payment card",
            Self::OneTimePassword => "Masked: one-time code",
            Self::RecoveryCode => "Masked: recovery code",
            Self::PossibleSecret => "Masked: possible secret",
        }
    }
}

/// Screen-space rectangle associated with the copied selection, when known.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Confidence that a clipboard backend's provenance evidence is authoritative.
///
/// Backends that cannot prove source attribution (for example the generic
/// `arboard` fallback) report [`ProvenanceConfidence::Unknown`]. `Unknown`
/// must never be treated as "proven clean": source-dependent rules cannot be
/// evaluated against evidence the backend did not supply, so policy degrades
/// according to the configured unknown-evidence mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceConfidence {
    /// A native backend supplied verifiable source attribution.
    Proven,
    /// The backend cannot prove source attribution. Fail-closed default.
    #[default]
    Unknown,
}

/// OS concealment evidence for one clipboard read.
///
/// This is a three-state signal on purpose: the legacy boolean collapsed
/// "the backend cannot read concealment markers" into `false`, which was then
/// interpreted as "proven clear". [`ConcealmentSignal::Unknown`] keeps that
/// uncertainty explicit so policy can degrade deliberately.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConcealmentSignal {
    /// The OS marked this clipboard content concealed/transient.
    Concealed,
    /// The backend affirmatively observed no concealment marker.
    Clear,
    /// The backend cannot read concealment markers. Fail-closed default.
    #[default]
    Unknown,
}

/// Whether one clipboard read saw a single, stable clipboard generation.
///
/// Three states for the same reason as [`ConcealmentSignal`]: the legacy
/// boolean had no way to say "this backend cannot observe owner/generation
/// identity", so that case was reported as `true` — an affirmative claim of
/// coherence nobody verified, which then travelled on into the capture gate
/// and the forensic ring as if it were evidence.
/// [`GenerationCoherence::Unknown`] keeps "held stable" and "nobody could
/// look" apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationCoherence {
    /// The backend held the clipboard owner/generation stable while every
    /// flavor was materialized.
    Coherent,
    /// The backend observed the owner or generation change mid-read: the
    /// flavor set may splice two different copies.
    Torn,
    /// The backend cannot observe owner/generation identity. Fail-closed
    /// default.
    #[default]
    Unknown,
}

/// Whether a selection change carried evidence of deliberate user intent.
///
/// Only consulted for [`SelectionSource::Primary`]: the X11/Wayland PRIMARY
/// selection changes on every mouse drag, so capturing it without positive
/// intent evidence records text the user never copied. The legacy boolean
/// reported "no evidence either way" as `true`, i.e. as an observed intent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionIntent {
    /// The backend observed a deliberate copy: the selection settled and an
    /// intent signal (gesture, key, ownership dwell) was seen.
    Intended,
    /// The backend affirmatively observed no intent signal — an incidental
    /// selection.
    Incidental,
    /// The backend cannot observe intent signals. Fail-closed default.
    #[default]
    Unknown,
}

/// Source context captured at the same instant as the clipboard generation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureProvenance {
    pub app_id: Option<String>,
    pub window_title: Option<String>,
    pub document_path: Option<String>,
    pub source_url: Option<String>,
    pub selection_rect: Option<SelectionRect>,
}

/// Monotonic identity supplied by a native clipboard backend.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureGeneration {
    pub epoch: u64,
    pub sequence: u64,
}

/// Origin information used to suppress local and cross-tool clipboard echoes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureLineage {
    pub origin_device: Option<String>,
    pub write_nonce: Option<String>,
}

/// Whether the bytes came directly from the source or were synthesized later.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlavorOrigin {
    #[default]
    Source,
    OsSynthesized,
    VbuffDerived,
}

/// Outcome of realizing a promised/delayed clipboard representation.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum FlavorRealization {
    #[default]
    Realized,
    Deferred,
    Failed,
    Truncated,
}

/// The payload of a single flavor.
///
/// Small payloads are stored `Inline`; large payloads are `Spilled` to an
/// out-of-row content-addressable file referenced by its BLAKE3 hex digest.
/// The MVP store keeps everything inline, but the variant exists so larger
/// payloads can be spilled later without changing the data model.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Body {
    /// The bytes live directly in the row.
    Inline(Vec<u8>),
    /// The bytes live in the CAS, keyed by this BLAKE3 hex digest.
    Spilled { blob_ref: String, byte_size: u64 },
}

impl Body {
    /// Number of bytes the payload occupies (inline length or spilled size).
    pub fn byte_size(&self) -> u64 {
        match self {
            Body::Inline(b) => b.len() as u64,
            Body::Spilled { byte_size, .. } => *byte_size,
        }
    }

    /// Borrow the inline bytes, if this body is inline.
    pub fn inline_bytes(&self) -> Option<&[u8]> {
        match self {
            Body::Inline(b) => Some(b),
            Body::Spilled { .. } => None,
        }
    }
}

impl fmt::Debug for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Body::Inline(b) => write!(f, "Inline({} bytes)", b.len()),
            Body::Spilled {
                blob_ref,
                byte_size,
            } => write!(f, "Spilled({blob_ref}, {byte_size} bytes)"),
        }
    }
}

/// One MIME representation of a clip, stored byte-for-byte.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flavor {
    /// Canonical MIME type, e.g. `text/plain;charset=utf-8` or `image/png`.
    pub mime: String,
    /// The payload bytes (inline or spilled).
    pub body: Body,
    /// Where the representation came from.
    #[serde(default)]
    pub origin: FlavorOrigin,
    /// Whether all promised bytes were materialized successfully.
    #[serde(default)]
    pub realization: FlavorRealization,
    /// Optional per-flavor BLAKE3 digest computed at the capture boundary.
    #[serde(default)]
    pub integrity_hash: Option<[u8; 32]>,
}

impl Flavor {
    /// Construct a flavor with an inline body.
    pub fn inline(mime: impl Into<String>, bytes: Vec<u8>) -> Self {
        Flavor {
            mime: mime.into(),
            body: Body::Inline(bytes),
            origin: FlavorOrigin::Source,
            realization: FlavorRealization::Realized,
            integrity_hash: None,
        }
    }

    /// Construct a representation derived by vbuff from canonical bytes.
    pub fn derived(mime: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            origin: FlavorOrigin::VbuffDerived,
            ..Self::inline(mime, bytes)
        }
    }

    /// True only when all promised bytes were read successfully.
    pub fn is_realized(&self) -> bool {
        self.realization == FlavorRealization::Realized
    }

    /// True if this flavor is a `text/*` flavor.
    pub fn is_text(&self) -> bool {
        let mime = self.mime_essence();
        mime.eq_ignore_ascii_case("text")
            || mime
                .get(..5)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("text/"))
    }

    /// True for the canonical plain-text representation, with optional MIME parameters.
    pub fn is_plain_text(&self) -> bool {
        let mime = self.mime_essence();
        mime.eq_ignore_ascii_case("text/plain") || mime.eq_ignore_ascii_case("text")
    }

    /// True if this flavor is an `image/*` flavor.
    pub fn is_image(&self) -> bool {
        self.mime_essence()
            .get(..6)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("image/"))
    }

    /// Interpret the inline bytes as UTF-8 text, if possible.
    pub fn as_text(&self) -> Option<&str> {
        self.body
            .inline_bytes()
            .and_then(|b| std::str::from_utf8(b).ok())
    }

    fn mime_essence(&self) -> &str {
        self.mime.split(';').next().unwrap_or(&self.mime).trim()
    }
}

/// Per-clip metadata captured at copy time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipMeta {
    /// When the clip was first captured.
    pub created_at: DateTime<Utc>,
    /// When the clip was last touched: bumped on a dedup re-copy (the store's
    /// "move to top" behavior). Equal to `created_at` until the first bump.
    /// This is the field eviction/recency policy should sort by, never
    /// `created_at` alone and never the clip id, since a clip's id is fixed
    /// at first capture and does not change when a repeat copy bumps it.
    pub updated_at: DateTime<Utc>,
    /// Total byte size across all flavors.
    pub byte_size: u64,
    /// Source application identifier, if known (bundle id / exe / WM_CLASS).
    pub source_app: Option<String>,
    /// Detected primary content kind.
    pub kind: ContentKind,
    /// Rich source context, when the platform can provide it.
    #[serde(default)]
    pub provenance: CaptureProvenance,
    /// Confidence that `provenance` is authoritative. Older databases
    /// deserialize this as `Unknown` (fail closed).
    #[serde(default)]
    pub provenance_confidence: ProvenanceConfidence,
    /// Native clipboard generation identity, when available.
    #[serde(default)]
    pub generation: Option<CaptureGeneration>,
    /// Write lineage used for echo suppression and future sync provenance.
    #[serde(default)]
    pub lineage: CaptureLineage,
    /// Hard expiry for ephemeral captures such as one-time codes.
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    /// Sensitive clips receive masked presentation and stricter retention.
    #[serde(default)]
    pub sensitive: bool,
    /// Payload-free explanation shown while sensitive content is masked.
    #[serde(default)]
    pub sensitivity_reason: Option<SensitivityReason>,
    /// False means the clip must never enter a sync envelope.
    #[serde(default = "default_true")]
    pub sync_eligible: bool,
    /// True only when the capture gate explicitly permits local inference.
    /// Missing values from older databases fail closed.
    #[serde(default)]
    pub ai_allowed: bool,
}

impl ClipMeta {
    /// Build metadata stamped at the current time for the given kind/size.
    pub fn now(kind: ContentKind, byte_size: u64, source_app: Option<String>) -> Self {
        let now = Utc::now();
        let provenance = CaptureProvenance {
            app_id: source_app.clone(),
            ..CaptureProvenance::default()
        };
        ClipMeta {
            created_at: now,
            updated_at: now,
            byte_size,
            source_app,
            kind,
            provenance,
            provenance_confidence: ProvenanceConfidence::Unknown,
            generation: None,
            lineage: CaptureLineage::default(),
            expires_at: None,
            sensitive: false,
            sensitivity_reason: None,
            sync_eligible: true,
            ai_allowed: false,
        }
    }
}

const fn default_true() -> bool {
    true
}

/// One logical copy event, holding every simultaneous flavor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clip {
    /// Unique, time-sortable id.
    pub id: ClipId,
    /// Every MIME flavor offered with this copy.
    pub flavors: Vec<Flavor>,
    /// BLAKE3 digest over the canonical flavor bytes; the dedup key.
    pub content_hash: [u8; 32],
    /// Captured metadata.
    pub meta: ClipMeta,
    /// Pinned clips are exempt from eviction and float to the top.
    pub pinned: bool,
    /// Marked a favorite by the user.
    pub favorite: bool,
}

impl Clip {
    /// The first text flavor's content, if any (used for previews/search).
    pub fn primary_text(&self) -> Option<&str> {
        self.flavors
            .iter()
            .find_map(|f| if f.is_text() { f.as_text() } else { None })
    }

    /// The first image flavor, if any.
    pub fn primary_image(&self) -> Option<&Flavor> {
        self.flavors.iter().find(|f| f.is_image())
    }

    /// A short, single-line preview suitable for a list row.
    pub fn preview(&self, max_chars: usize) -> String {
        if let Some(text) = self.primary_text() {
            let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
            truncate_chars(&collapsed, max_chars)
        } else if self.primary_image().is_some() {
            format!("[image, {} bytes]", self.meta.byte_size)
        } else {
            match self.flavors.first() {
                Some(f) => format!("[{}]", f.mime),
                None => "[empty]".to_string(),
            }
        }
    }

    /// The content hash rendered as a lowercase hex string.
    pub fn content_hash_hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in &self.content_hash {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }
}

/// Truncate a string to at most `max_chars` characters, appending an ellipsis
/// if it was shortened.
pub fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_id_roundtrips() {
        let id = ClipId::new();
        let s = id.to_string_repr();
        let parsed = ClipId::parse(&s).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn preview_collapses_whitespace() {
        let clip = Clip {
            id: ClipId::new(),
            flavors: vec![Flavor::inline("text/plain", b"hello\n  world  ".to_vec())],
            content_hash: [0u8; 32],
            meta: ClipMeta::now(ContentKind::Text, 0, None),
            pinned: false,
            favorite: false,
        };
        assert_eq!(clip.preview(100), "hello world");
    }

    #[test]
    fn mime_helpers_ignore_case_and_parameters() {
        let text = Flavor::inline(" TEXT/PLAIN; charset=utf-8 ", b"hello".to_vec());
        let image = Flavor::inline("IMAGE/PNG", vec![1, 2, 3]);

        assert!(text.is_text());
        assert!(text.is_plain_text());
        assert!(image.is_image());
    }

    #[test]
    fn truncate_adds_ellipsis() {
        assert_eq!(truncate_chars("abcdef", 4), "abc…");
        assert_eq!(truncate_chars("ab", 4), "ab");
    }

    #[test]
    fn evidence_signals_default_fail_closed_and_roundtrip_snake_case() {
        assert_eq!(
            ProvenanceConfidence::default(),
            ProvenanceConfidence::Unknown
        );
        assert_eq!(ConcealmentSignal::default(), ConcealmentSignal::Unknown);
        assert_eq!(GenerationCoherence::default(), GenerationCoherence::Unknown);
        assert_eq!(SelectionIntent::default(), SelectionIntent::Unknown);
        assert_eq!(
            serde_json::to_string(&ProvenanceConfidence::Proven).unwrap(),
            r#""proven""#
        );
        assert_eq!(
            serde_json::to_string(&ConcealmentSignal::Concealed).unwrap(),
            r#""concealed""#
        );
        assert_eq!(
            serde_json::from_str::<ConcealmentSignal>(r#""clear""#).unwrap(),
            ConcealmentSignal::Clear
        );
        assert_eq!(
            serde_json::to_string(&GenerationCoherence::Torn).unwrap(),
            r#""torn""#
        );
        assert_eq!(
            serde_json::from_str::<GenerationCoherence>(r#""coherent""#).unwrap(),
            GenerationCoherence::Coherent
        );
        assert_eq!(
            serde_json::to_string(&SelectionIntent::Incidental).unwrap(),
            r#""incidental""#
        );
        assert_eq!(
            serde_json::from_str::<SelectionIntent>(r#""intended""#).unwrap(),
            SelectionIntent::Intended
        );
    }

    #[test]
    fn content_kind_discriminant_roundtrips_for_every_kind() {
        for &kind in ContentKind::ALL {
            assert_eq!(
                ContentKind::from_stored_discriminant(kind.stored_discriminant()),
                Some(kind)
            );
        }
    }

    #[test]
    fn content_kind_discriminants_and_slugs_are_pinned_database_format() {
        // These exact pairs are the on-disk format (SQLite `clips.kind`,
        // retention rows, CAS shard directories). Renumbering or renaming is
        // a breaking schema change; this test must only ever gain lines.
        let pinned: [(ContentKind, i64, &str); 9] = [
            (ContentKind::Text, 0, "text"),
            (ContentKind::Rtf, 1, "rtf"),
            (ContentKind::Html, 2, "html"),
            (ContentKind::Image, 3, "image"),
            (ContentKind::File, 4, "file"),
            (ContentKind::Color, 5, "color"),
            (ContentKind::Url, 6, "url"),
            (ContentKind::Code, 7, "code"),
            (ContentKind::Other, 8, "other"),
        ];
        for (kind, number, slug) in pinned {
            assert_eq!(kind.stored_discriminant(), number);
            assert_eq!(kind.slug(), slug);
        }
    }

    #[test]
    fn content_kind_unknown_discriminant_fails_closed() {
        for value in [-1, 9, 42, 255, i64::MIN, i64::MAX] {
            assert_eq!(ContentKind::from_stored_discriminant(value), None);
        }
    }

    #[test]
    fn content_kind_slug_roundtrips_via_from_str() {
        for &kind in ContentKind::ALL {
            assert_eq!(kind.slug().parse::<ContentKind>(), Ok(kind));
        }
    }

    #[test]
    fn content_kind_from_str_rejects_unknown_and_non_canonical() {
        for input in ["", "TEXT", "texts", " text", "text ", "unknown", "7"] {
            assert_eq!(
                input.parse::<ContentKind>(),
                Err(ParseContentKindError),
                "input {input:?} must be rejected"
            );
        }
    }

    #[test]
    fn content_kind_all_covers_every_variant() {
        // Compile-time anchor: adding a ContentKind variant makes this match
        // non-exhaustive and fails the build, forcing ALL (and thereby the
        // discriminant and slug codecs) to be extended in the same change.
        for &kind in ContentKind::ALL {
            match kind {
                ContentKind::Text
                | ContentKind::Url
                | ContentKind::Color
                | ContentKind::Code
                | ContentKind::Image
                | ContentKind::File
                | ContentKind::Rtf
                | ContentKind::Html
                | ContentKind::Other => {}
            }
        }
        // Every variant appears in ALL exactly once.
        for probe in [
            ContentKind::Text,
            ContentKind::Url,
            ContentKind::Color,
            ContentKind::Code,
            ContentKind::Image,
            ContentKind::File,
            ContentKind::Rtf,
            ContentKind::Html,
            ContentKind::Other,
        ] {
            assert_eq!(
                ContentKind::ALL
                    .iter()
                    .filter(|kind| **kind == probe)
                    .count(),
                1,
                "{probe:?} must appear in ContentKind::ALL exactly once"
            );
        }
        assert_eq!(ContentKind::ALL.len(), 9);
    }

    #[test]
    fn clip_meta_from_older_database_fails_closed_on_provenance_confidence() {
        let meta = ClipMeta::now(ContentKind::Text, 1, None);
        let json = serde_json::to_value(&meta).unwrap();
        // Simulate a database row written before the field existed.
        let mut older = json.clone();
        older
            .as_object_mut()
            .unwrap()
            .remove("provenance_confidence");
        let restored: ClipMeta = serde_json::from_value(older).unwrap();
        assert_eq!(
            restored.provenance_confidence,
            ProvenanceConfidence::Unknown
        );
    }
}
