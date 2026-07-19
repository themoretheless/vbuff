//! Deduplication, recovery, and retention primitives.
//!
//! This module owns content lifecycle policy while `Store` remains responsible
//! for SQLite transactions and CAS hydration.

use std::time::Duration;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use vbuff_core::content_hash_from_flavors;
use vbuff_types::{Clip, ClipId, ContentKind};
use zeroize::Zeroizing;

use crate::{Result, StoreError};

const NORMALIZED_INPUT_LIMIT: usize = 1024 * 1024;
const GRACE_PAYLOAD_LIMIT: usize = 512 * 1024 * 1024;
const GRACE_DOMAIN: &[u8] = b"vbuff-grace-bin-v1";
pub(crate) const MAX_RETENTION_EVICTIONS: usize = 10_000;

/// One exact-content re-copy recorded without retaining another payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MergeLedgerEntry {
    pub clip_id: ClipId,
    pub merged_at: DateTime<Utc>,
}

/// A frequently reused row that has not yet been pinned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SuggestedPin {
    pub clip_id: ClipId,
    pub reuse_count: u64,
    pub last_reused_at: DateTime<Utc>,
}

/// Why a clip entered the encrypted grace bin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeletionReason {
    User,
    HistoryCap,
    Retention,
}

impl DeletionReason {
    pub(crate) const fn as_i64(self) -> i64 {
        match self {
            Self::User => 0,
            Self::HistoryCap => 1,
            Self::Retention => 2,
        }
    }

    pub(crate) fn from_i64(value: i64) -> Result<Self> {
        match value {
            0 => Ok(Self::User),
            1 => Ok(Self::HistoryCap),
            2 => Ok(Self::Retention),
            _ => Err(StoreError::Corrupt("invalid grace-bin reason".into())),
        }
    }
}

/// Content-free metadata for a recoverable encrypted deletion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GraceBinEntry {
    pub recovery_id: String,
    pub clip_id: ClipId,
    pub deleted_at: DateTime<Utc>,
    pub purge_after: DateTime<Utc>,
    pub reason: DeletionReason,
}

/// A class that can receive an independent retention rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RetentionScope {
    Kind(ContentKind),
    Sensitive,
}

impl RetentionScope {
    pub(crate) const fn database_values(self) -> (i64, bool) {
        match self {
            Self::Kind(kind) => (kind_to_int(kind), false),
            Self::Sensitive => (-1, true),
        }
    }

    pub(crate) fn from_database(kind: i64, sensitive: bool) -> Result<Self> {
        if sensitive {
            if kind == -1 {
                return Ok(Self::Sensitive);
            }
            return Err(StoreError::Corrupt(
                "invalid sensitive retention scope".into(),
            ));
        }
        Ok(Self::Kind(kind_from_int(kind)?))
    }
}

/// A bounded retention rule. `None` means that dimension is unlimited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetentionRule {
    pub scope: RetentionScope,
    pub max_age: Option<Duration>,
    pub max_items: Option<usize>,
    pub grace_window: Duration,
}

impl RetentionRule {
    pub fn validate(&self) -> Result<()> {
        const TEN_YEARS: Duration = Duration::from_secs(10 * 365 * 24 * 60 * 60);
        const SEVEN_DAYS: Duration = Duration::from_secs(7 * 24 * 60 * 60);
        if self.max_age.is_none() && self.max_items.is_none() {
            return Err(StoreError::Maintenance(
                "retention rule must bound age or item count".into(),
            ));
        }
        if self
            .max_age
            .is_some_and(|age| age.is_zero() || age > TEN_YEARS)
            || self.max_items.is_some_and(|items| items > 1_000_000)
            || self.grace_window > SEVEN_DAYS
        {
            return Err(StoreError::Maintenance(
                "retention rule exceeds lifecycle bounds".into(),
            ));
        }
        Ok(())
    }
}

/// Result of one bounded retention maintenance pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RetentionReport {
    pub encrypted: usize,
    pub hard_deleted: usize,
    pub deferred_without_key: usize,
    pub remaining_candidates: usize,
}

pub(crate) fn default_retention_rules() -> Vec<RetentionRule> {
    const DAY: u64 = 24 * 60 * 60;
    fn rule(kind: ContentKind, days: u64, items: usize, grace_hours: u64) -> RetentionRule {
        RetentionRule {
            scope: RetentionScope::Kind(kind),
            max_age: Some(Duration::from_secs(days * DAY)),
            max_items: Some(items),
            grace_window: Duration::from_secs(grace_hours * 60 * 60),
        }
    }

    vec![
        rule(ContentKind::Text, 180, 10_000, 24),
        rule(ContentKind::Code, 180, 5_000, 24),
        rule(ContentKind::Url, 90, 3_000, 24),
        rule(ContentKind::Color, 180, 1_000, 24),
        rule(ContentKind::Image, 14, 500, 24),
        rule(ContentKind::File, 14, 500, 24),
        rule(ContentKind::Rtf, 60, 2_000, 24),
        rule(ContentKind::Html, 60, 2_000, 24),
        rule(ContentKind::Other, 30, 1_000, 24),
        RetentionRule {
            scope: RetentionScope::Sensitive,
            max_age: Some(Duration::from_secs(15 * 60)),
            max_items: Some(50),
            // Sensitive TTL is a privacy boundary, so the default is hard delete.
            grace_window: Duration::ZERO,
        },
    ]
}

/// Normalize cosmetic text differences and return a domain-separated digest.
/// Exact bytes remain canonical and are never replaced by this fingerprint.
pub fn normalized_text_fingerprint(text: &str) -> Option<[u8; 32]> {
    if text.is_empty() || text.len() > NORMALIZED_INPUT_LIMIT {
        return None;
    }
    let mut normalized = String::with_capacity(text.len());
    let mut pending_space = false;
    for original in text.chars() {
        if original.is_whitespace() {
            pending_space = !normalized.is_empty();
            continue;
        }
        let character = canonical_punctuation(original);
        if character.is_alphanumeric() {
            let joins_without_space = normalized
                .chars()
                .next_back()
                .is_some_and(|last| matches!(last, ' ' | '-' | '/' | '\'' | '"'));
            if pending_space && !joins_without_space {
                normalized.push(' ');
            }
            for lower in character.to_lowercase() {
                normalized.push(lower);
            }
        } else {
            while normalized.ends_with(' ') {
                normalized.pop();
            }
            if character == '-' && normalized.ends_with('-') {
                pending_space = false;
                continue;
            }
            normalized.push(character);
        }
        pending_space = false;
    }
    while normalized.ends_with(' ') {
        normalized.pop();
    }
    if normalized.is_empty() {
        return None;
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vbuff-normalized-text-v1\0");
    hasher.update(normalized.as_bytes());
    Some(*hasher.finalize().as_bytes())
}

fn canonical_punctuation(character: char) -> char {
    match character {
        '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}' => '\'',
        '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{201f}' => '"',
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2212}' => '-',
        '\u{00a0}' | '\u{2007}' | '\u{202f}' => ' ',
        other => other,
    }
}

pub(crate) fn seal_clip(
    key: &[u8; 32],
    recovery_id: &str,
    clip: &Clip,
    deleted_at_ms: i64,
    purge_after_ms: i64,
    reason: DeletionReason,
) -> Result<([u8; 24], Vec<u8>)> {
    let plaintext = Zeroizing::new(serde_json::to_vec(clip)?);
    if plaintext.len() > GRACE_PAYLOAD_LIMIT {
        return Err(StoreError::Maintenance(
            "clip is too large for the encrypted grace bin".into(),
        ));
    }
    let mut nonce = [0_u8; 24];
    getrandom::fill(&mut nonce).map_err(|_| StoreError::Crypto)?;
    let aad = grace_aad(
        recovery_id,
        &clip.id.to_string_repr(),
        deleted_at_ms,
        purge_after_ms,
        reason,
    );
    let ciphertext = XChaCha20Poly1305::new(key.into())
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: plaintext.as_slice(),
                aad: &aad,
            },
        )
        .map_err(|_| StoreError::Crypto)?;
    Ok((nonce, ciphertext))
}

#[derive(Clone, Copy)]
pub(crate) struct EncryptedGraceRecord<'a> {
    pub recovery_id: &'a str,
    pub clip_id: &'a str,
    pub deleted_at_ms: i64,
    pub purge_after_ms: i64,
    pub reason: DeletionReason,
    pub nonce: &'a [u8],
    pub ciphertext: &'a [u8],
}

pub(crate) fn open_clip(key: &[u8; 32], record: &EncryptedGraceRecord<'_>) -> Result<Clip> {
    let nonce: [u8; 24] = record
        .nonce
        .try_into()
        .map_err(|_| StoreError::Corrupt("invalid grace-bin nonce".into()))?;
    if record.ciphertext.len() < 16 || record.ciphertext.len() > GRACE_PAYLOAD_LIMIT + 16 {
        return Err(StoreError::Corrupt(
            "invalid grace-bin ciphertext size".into(),
        ));
    }
    let aad = grace_aad(
        record.recovery_id,
        record.clip_id,
        record.deleted_at_ms,
        record.purge_after_ms,
        record.reason,
    );
    let plaintext = Zeroizing::new(
        XChaCha20Poly1305::new(key.into())
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: record.ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| StoreError::Crypto)?,
    );
    if plaintext.len() > GRACE_PAYLOAD_LIMIT {
        return Err(StoreError::Corrupt("grace-bin payload is too large".into()));
    }
    let clip: Clip = serde_json::from_slice(plaintext.as_slice())?;
    if clip.id.to_string_repr() != record.clip_id
        || content_hash_from_flavors(&clip.flavors) != clip.content_hash
    {
        return Err(StoreError::Corrupt(
            "grace-bin payload identity check failed".into(),
        ));
    }
    Ok(clip)
}

fn grace_aad(
    recovery_id: &str,
    clip_id: &str,
    deleted_at_ms: i64,
    purge_after_ms: i64,
    reason: DeletionReason,
) -> Vec<u8> {
    let mut aad = Vec::with_capacity(GRACE_DOMAIN.len() + recovery_id.len() + clip_id.len() + 25);
    aad.extend_from_slice(GRACE_DOMAIN);
    aad.push(0);
    aad.extend_from_slice(recovery_id.as_bytes());
    aad.push(0);
    aad.extend_from_slice(clip_id.as_bytes());
    aad.extend_from_slice(&deleted_at_ms.to_be_bytes());
    aad.extend_from_slice(&purge_after_ms.to_be_bytes());
    aad.extend_from_slice(&reason.as_i64().to_be_bytes());
    aad
}

const fn kind_to_int(kind: ContentKind) -> i64 {
    match kind {
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

fn kind_from_int(value: i64) -> Result<ContentKind> {
    match value {
        0 => Ok(ContentKind::Text),
        1 => Ok(ContentKind::Rtf),
        2 => Ok(ContentKind::Html),
        3 => Ok(ContentKind::Image),
        4 => Ok(ContentKind::File),
        5 => Ok(ContentKind::Color),
        6 => Ok(ContentKind::Url),
        7 => Ok(ContentKind::Code),
        8 => Ok(ContentKind::Other),
        _ => Err(StoreError::Corrupt("invalid retention kind".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_types::{ClipMeta, Flavor};

    fn clip(text: &str) -> Clip {
        let flavors = vec![Flavor::inline("text/plain", text.as_bytes().to_vec())];
        Clip {
            id: ClipId::new(),
            content_hash: content_hash_from_flavors(&flavors),
            meta: ClipMeta::now(ContentKind::Text, text.len() as u64, None),
            flavors,
            pinned: false,
            favorite: false,
        }
    }

    #[test]
    fn cosmetic_text_variants_share_a_fingerprint() {
        assert_eq!(
            normalized_text_fingerprint("Hello,\nworld -- next"),
            normalized_text_fingerprint(" hello , world \u{2014} next ")
        );
        assert_ne!(
            normalized_text_fingerprint("hello world"),
            normalized_text_fingerprint("hello brave world")
        );
    }

    #[test]
    fn grace_ciphertext_roundtrips_and_authenticates_metadata() {
        let source = clip("recover me");
        let key = [7_u8; 32];
        let recovery_id = ClipId::new().to_string_repr();
        let (nonce, ciphertext) =
            seal_clip(&key, &recovery_id, &source, 100, 200, DeletionReason::User).unwrap();
        assert!(!ciphertext.windows(10).any(|window| window == b"recover me"));
        let clip_id = source.id.to_string_repr();
        let record = EncryptedGraceRecord {
            recovery_id: &recovery_id,
            clip_id: &clip_id,
            deleted_at_ms: 100,
            purge_after_ms: 200,
            reason: DeletionReason::User,
            nonce: &nonce,
            ciphertext: &ciphertext,
        };
        let restored = open_clip(&key, &record).unwrap();
        assert_eq!(restored, source);
        let tampered = EncryptedGraceRecord {
            deleted_at_ms: 101,
            ..record
        };
        assert!(open_clip(&key, &tampered).is_err());
        assert!(open_clip(&[8; 32], &record).is_err());
    }

    #[test]
    fn default_retention_rules_cover_every_kind_and_sensitive_content() {
        let rules = default_retention_rules();
        assert_eq!(rules.len(), 10);
        assert!(rules.iter().all(|rule| rule.validate().is_ok()));
        assert!(rules.iter().any(|rule| {
            rule.scope == RetentionScope::Sensitive && rule.grace_window.is_zero()
        }));
    }
}
