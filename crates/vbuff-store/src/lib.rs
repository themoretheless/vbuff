//! SQLite-backed persistence for vbuff's clip history.
//!
//! The MVP schema is deliberately compact: a single `clips` table whose
//! `flavors` column holds the flavor set as a JSON blob. This keeps reads to a
//! single row fetch and avoids a join on the hot path; the full normalized
//! `item`/`flavor` split from the architecture can be migrated to later.
//!
//! The database lives at `dirs::data_dir()/vbuff/history.db`, runs in WAL mode,
//! and is opened by a single owner. Inserts are dedup-aware: re-copying
//! identical content bumps the existing row to the top instead of inserting a
//! duplicate.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use vbuff_types::{Clip, ClipId, ClipMeta, ContentKind, Flavor};

mod error;
mod serde_clip;

pub use error::StoreError;

/// Result type for store operations.
pub type Result<T> = std::result::Result<T, StoreError>;

/// The current schema version, stored in `PRAGMA user_version`.
const SCHEMA_VERSION: i64 = 1;

/// A handle to the clip-history database.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (creating if necessary) the store at the default data path:
    /// `<data_dir>/vbuff/history.db`.
    pub fn open_default() -> Result<Self> {
        let path = default_db_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(StoreError::Io)?;
        }
        Self::open(&path)
    }

    /// Open (creating if necessary) the store at a specific path.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    /// Open an in-memory store (useful for tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let mut store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    /// Apply forward-only migrations based on `user_version`.
    fn migrate(&mut self) -> Result<()> {
        let version: i64 =
            self.conn
                .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 1 {
            self.conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS clips (
                    seq          INTEGER PRIMARY KEY AUTOINCREMENT, -- definitive recency tiebreaker
                    id           TEXT NOT NULL UNIQUE,    -- ULID string
                    content_hash BLOB NOT NULL,           -- 32-byte BLAKE3 digest
                    flavors      TEXT NOT NULL,           -- JSON array of flavors
                    kind         INTEGER NOT NULL,        -- ContentKind discriminant
                    created_at   INTEGER NOT NULL,        -- epoch millis (UTC)
                    updated_at   INTEGER NOT NULL,        -- bumped on re-copy (move to top)
                    byte_size    INTEGER NOT NULL,
                    source_app   TEXT,
                    preview      TEXT NOT NULL DEFAULT '',-- cached search/preview text
                    pinned       INTEGER NOT NULL DEFAULT 0,
                    favorite     INTEGER NOT NULL DEFAULT 0
                );
                CREATE UNIQUE INDEX IF NOT EXISTS idx_clips_hash ON clips(content_hash);
                CREATE INDEX IF NOT EXISTS idx_clips_updated ON clips(updated_at DESC, seq DESC);
                CREATE INDEX IF NOT EXISTS idx_clips_pinned ON clips(updated_at DESC) WHERE pinned = 1;
                "#,
            )?;
            self.conn
                .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(())
    }

    /// Insert a clip, deduplicating by content hash.
    ///
    /// If a clip with the same `content_hash` already exists, its `updated_at`
    /// is bumped to now (moving it to the top) and its existing [`ClipId`] is
    /// returned. Otherwise the new clip is inserted and its id returned.
    pub fn insert(&self, clip: &Clip) -> Result<ClipId> {
        let now = now_millis();

        // Dedup: does this content already exist?
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM clips WHERE content_hash = ?1",
                params![clip.content_hash.as_slice()],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(id_str) = existing {
            // Bump both updated_at and seq so the deduped clip floats to the top
            // even when several inserts share the same millisecond.
            self.conn.execute(
                "UPDATE clips SET updated_at = ?1, seq = (SELECT COALESCE(MAX(seq), 0) + 1 FROM clips) WHERE id = ?2",
                params![now, id_str],
            )?;
            return ClipId::parse(&id_str).map_err(|_| StoreError::Corrupt("bad ulid in db".into()));
        }

        let flavors_json = serde_clip::flavors_to_json(&clip.flavors)?;
        let created = clip.meta.created_at.timestamp_millis();
        let preview = clip.preview(512);

        self.conn.execute(
            r#"
            INSERT INTO clips
                (id, content_hash, flavors, kind, created_at, updated_at,
                 byte_size, source_app, preview, pinned, favorite)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                clip.id.to_string_repr(),
                clip.content_hash.as_slice(),
                flavors_json,
                kind_to_int(clip.meta.kind),
                created,
                now,
                clip.meta.byte_size as i64,
                clip.meta.source_app,
                preview,
                clip.pinned as i64,
                clip.favorite as i64,
            ],
        )?;
        Ok(clip.id)
    }

    /// List the most recent clips (pinned first, then by recency), up to `limit`.
    pub fn list(&self, limit: usize) -> Result<Vec<Clip>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, content_hash, flavors, kind, created_at, updated_at,
                   byte_size, source_app, pinned, favorite
            FROM clips
            ORDER BY pinned DESC, updated_at DESC, seq DESC
            LIMIT ?1
            "#,
        )?;
        let rows = stmt.query_map(params![limit as i64], row_to_clip)?;
        collect_clips(rows)
    }

    /// Load the most recent clips (alias used at startup to hydrate the GUI).
    pub fn load_recent(&self, limit: usize) -> Result<Vec<Clip>> {
        self.list(limit)
    }

    /// Search clips by a case-insensitive substring over the cached preview.
    ///
    /// Ranking is left to `vbuff-core::search` on the caller side; this method
    /// just narrows the candidate set with SQL `LIKE`.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Clip>> {
        if query.trim().is_empty() {
            return self.list(limit);
        }
        let pattern = format!("%{}%", escape_like(query));
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, content_hash, flavors, kind, created_at, updated_at,
                   byte_size, source_app, pinned, favorite
            FROM clips
            WHERE preview LIKE ?1 ESCAPE '\' OR source_app LIKE ?1 ESCAPE '\'
            ORDER BY pinned DESC, updated_at DESC, seq DESC
            LIMIT ?2
            "#,
        )?;
        let rows = stmt.query_map(params![pattern, limit as i64], row_to_clip)?;
        collect_clips(rows)
    }

    /// Set (or clear) the pinned flag on a clip.
    pub fn set_pinned(&self, id: ClipId, pinned: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE clips SET pinned = ?1 WHERE id = ?2",
            params![pinned as i64, id.to_string_repr()],
        )?;
        Ok(())
    }

    /// Set (or clear) the favorite flag on a clip.
    pub fn set_favorite(&self, id: ClipId, favorite: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE clips SET favorite = ?1 WHERE id = ?2",
            params![favorite as i64, id.to_string_repr()],
        )?;
        Ok(())
    }

    /// Delete a single clip by id.
    pub fn delete(&self, id: ClipId) -> Result<()> {
        self.conn.execute(
            "DELETE FROM clips WHERE id = ?1",
            params![id.to_string_repr()],
        )?;
        Ok(())
    }

    /// Delete every non-pinned clip. Pinned clips are preserved.
    pub fn clear(&self) -> Result<()> {
        self.conn.execute("DELETE FROM clips WHERE pinned = 0", [])?;
        Ok(())
    }

    /// Delete every clip, including pinned ones.
    pub fn clear_all(&self) -> Result<()> {
        self.conn.execute("DELETE FROM clips", [])?;
        Ok(())
    }

    /// Total number of stored clips.
    pub fn count(&self) -> Result<usize> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM clips", [], |row| row.get(0))?;
        Ok(n as usize)
    }

    /// Enforce a count cap, deleting oldest non-pinned/non-favorite clips first.
    ///
    /// Returns the number of clips evicted.
    pub fn enforce_cap(&self, max_history: usize) -> Result<usize> {
        let total = self.count()?;
        if total <= max_history {
            return Ok(0);
        }
        let overflow = total - max_history;
        let deleted = self.conn.execute(
            r#"
            DELETE FROM clips WHERE id IN (
                SELECT id FROM clips
                WHERE pinned = 0 AND favorite = 0
                ORDER BY updated_at ASC, seq ASC
                LIMIT ?1
            )
            "#,
            params![overflow as i64],
        )?;
        Ok(deleted)
    }
}

/// The default database path: `<data_dir>/vbuff/history.db`.
pub fn default_db_path() -> Result<PathBuf> {
    let dir = dirs_data_dir().ok_or_else(|| StoreError::NoDataDir)?;
    Ok(dir.join("vbuff").join("history.db"))
}

/// Resolve the platform data directory.
fn dirs_data_dir() -> Option<PathBuf> {
    dirs_next_data_dir()
}

// Avoid a hard `dirs` dependency in this crate by re-implementing the small bit
// we need via std + env fallbacks. The app crate uses `dirs` directly; here we
// keep the store dependency-light.
fn dirs_next_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|h| h.join("Library").join("Application Support"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            return Some(PathBuf::from(xdg));
        }
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|h| h.join(".local").join("share"))
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

fn collect_clips(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<RawRow>>,
) -> Result<Vec<Clip>> {
    let mut out = Vec::new();
    for row in rows {
        out.push(raw_to_clip(row?)?);
    }
    Ok(out)
}

/// Intermediate row representation before JSON decoding.
struct RawRow {
    id: String,
    content_hash: Vec<u8>,
    flavors_json: String,
    kind: i64,
    created_at: i64,
    byte_size: i64,
    source_app: Option<String>,
    pinned: bool,
    favorite: bool,
}

fn row_to_clip(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok(RawRow {
        id: row.get(0)?,
        content_hash: row.get(1)?,
        flavors_json: row.get(2)?,
        kind: row.get(3)?,
        created_at: row.get(4)?,
        // index 5 (updated_at) is used for ordering only.
        byte_size: row.get(6)?,
        source_app: row.get(7)?,
        pinned: row.get::<_, i64>(8)? != 0,
        favorite: row.get::<_, i64>(9)? != 0,
    })
}

fn raw_to_clip(raw: RawRow) -> Result<Clip> {
    let id = ClipId::parse(&raw.id).map_err(|_| StoreError::Corrupt("bad ulid in db".into()))?;
    let flavors: Vec<Flavor> = serde_clip::flavors_from_json(&raw.flavors_json)?;
    let mut content_hash = [0u8; 32];
    if raw.content_hash.len() == 32 {
        content_hash.copy_from_slice(&raw.content_hash);
    } else {
        return Err(StoreError::Corrupt("content_hash not 32 bytes".into()));
    }
    let created_at = chrono::DateTime::from_timestamp_millis(raw.created_at)
        .unwrap_or_else(chrono::Utc::now);
    let meta = ClipMeta {
        created_at,
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

fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Escape `%`, `_`, and `\` for a SQL `LIKE` pattern using `\` as the escape.
fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn kind_to_int(kind: ContentKind) -> i64 {
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

fn kind_from_int(v: i64) -> ContentKind {
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

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_core::content_hash_from_flavors;
    use vbuff_types::{ClipMeta, ContentKind, Flavor};

    fn make_clip(text: &str) -> Clip {
        let flavors = vec![Flavor::inline("text/plain", text.as_bytes().to_vec())];
        let content_hash = content_hash_from_flavors(&flavors);
        Clip {
            id: ClipId::new(),
            flavors,
            content_hash,
            meta: ClipMeta::now(ContentKind::Text, text.len() as u64, Some("test.app".into())),
            pinned: false,
            favorite: false,
        }
    }

    #[test]
    fn insert_and_list() {
        let store = Store::open_in_memory().unwrap();
        let c1 = make_clip("hello");
        let c2 = make_clip("world");
        store.insert(&c1).unwrap();
        store.insert(&c2).unwrap();
        assert_eq!(store.count().unwrap(), 2);
        let listed = store.list(10).unwrap();
        assert_eq!(listed.len(), 2);
        // Most recent insert (world) first.
        assert_eq!(listed[0].primary_text(), Some("world"));
    }

    #[test]
    fn dedup_bumps_existing() {
        let store = Store::open_in_memory().unwrap();
        let c1 = make_clip("dup");
        let id1 = store.insert(&c1).unwrap();
        // Insert different content, then re-insert the duplicate content.
        store.insert(&make_clip("other")).unwrap();
        let c1_again = make_clip("dup"); // same content, new id
        let id_again = store.insert(&c1_again).unwrap();
        // Dedup returns the original id, and no new row is added.
        assert_eq!(id1, id_again);
        assert_eq!(store.count().unwrap(), 2);
        // The deduped clip should now be on top (most recently updated).
        let listed = store.list(10).unwrap();
        assert_eq!(listed[0].primary_text(), Some("dup"));
    }

    #[test]
    fn pin_search_delete_clear() {
        let store = Store::open_in_memory().unwrap();
        let c = make_clip("findme please");
        let id = store.insert(&c).unwrap();
        store.insert(&make_clip("unrelated")).unwrap();

        // search
        let hits = store.search("findme", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].primary_text(), Some("findme please"));

        // pin then clear keeps pinned
        store.set_pinned(id, true).unwrap();
        store.clear().unwrap();
        assert_eq!(store.count().unwrap(), 1);
        assert!(store.list(10).unwrap()[0].pinned);

        // delete
        store.delete(id).unwrap();
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn enforce_cap_evicts_oldest_unpinned() {
        let store = Store::open_in_memory().unwrap();
        let pinned = make_clip("keep me pinned");
        let pid = store.insert(&pinned).unwrap();
        store.set_pinned(pid, true).unwrap();
        for i in 0..5 {
            store.insert(&make_clip(&format!("clip {i}"))).unwrap();
        }
        assert_eq!(store.count().unwrap(), 6);
        let evicted = store.enforce_cap(3).unwrap();
        assert_eq!(evicted, 3);
        assert_eq!(store.count().unwrap(), 3);
        // Pinned survived.
        assert!(store.list(10).unwrap().iter().any(|c| c.pinned));
    }
}
