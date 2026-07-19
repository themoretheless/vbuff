//! Mapping between SQLite rows and [`Clip`] values.

use vbuff_types::{Clip, ClipId, ClipMeta, ContentKind, Flavor};

use crate::{Result, StoreError, serde_clip};

/// Intermediate row representation before JSON decoding.
pub(crate) struct RawRow {
    pub id: String,
    pub content_hash: Vec<u8>,
    pub flavors_json: String,
    pub kind: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub byte_size: i64,
    pub source_app: Option<String>,
    pub pinned: bool,
    pub favorite: bool,
}

/// Map a query row (id, content_hash, flavors, kind, created_at, updated_at,
/// byte_size, source_app, pinned, favorite) into a [`RawRow`].
pub(crate) fn row_to_clip(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok(RawRow {
        id: row.get(0)?,
        content_hash: row.get(1)?,
        flavors_json: row.get(2)?,
        kind: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        byte_size: row.get(6)?,
        source_app: row.get(7)?,
        pinned: row.get::<_, i64>(8)? != 0,
        favorite: row.get::<_, i64>(9)? != 0,
    })
}

/// Decode a [`RawRow`] into a full [`Clip`], deserializing the flavor JSON.
pub(crate) fn raw_to_clip(raw: RawRow) -> Result<Clip> {
    let id = ClipId::parse(&raw.id).map_err(|_| StoreError::Corrupt("bad ulid in db".into()))?;
    let flavors: Vec<Flavor> = serde_clip::flavors_from_json(&raw.flavors_json)?;
    let mut content_hash = [0u8; 32];
    if raw.content_hash.len() == 32 {
        content_hash.copy_from_slice(&raw.content_hash);
    } else {
        return Err(StoreError::Corrupt("content_hash not 32 bytes".into()));
    }
    let created_at =
        chrono::DateTime::from_timestamp_millis(raw.created_at).unwrap_or_else(chrono::Utc::now);
    let updated_at = chrono::DateTime::from_timestamp_millis(raw.updated_at).unwrap_or(created_at);
    let meta = ClipMeta {
        created_at,
        updated_at,
        byte_size: raw.byte_size as u64,
        source_app: raw.source_app,
        kind: kind_from_int(raw.kind),
    };
    Ok(Clip {
        id,
        flavors,
        content_hash,
        meta,
        pinned: raw.pinned,
        favorite: raw.favorite,
    })
}

/// Collect a `RawRow` query result into decoded [`Clip`]s.
pub(crate) fn collect_clips(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<RawRow>>,
) -> Result<Vec<Clip>> {
    let mut out = Vec::new();
    for row in rows {
        out.push(raw_to_clip(row?)?);
    }
    Ok(out)
}

pub(crate) fn kind_to_int(kind: ContentKind) -> i64 {
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

pub(crate) fn kind_from_int(v: i64) -> ContentKind {
    match v {
        0 => ContentKind::Text,
        1 => ContentKind::Rtf,
        2 => ContentKind::Html,
        3 => ContentKind::Image,
        4 => ContentKind::File,
        5 => ContentKind::Color,
        6 => ContentKind::Url,
        7 => ContentKind::Code,
        _ => ContentKind::Other,
    }
}
