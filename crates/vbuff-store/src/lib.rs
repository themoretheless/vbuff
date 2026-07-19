//! SQLite-backed persistence for vbuff's clip history.
//!
//! The database lives at `dirs::data_dir()/vbuff/history.db`, runs in WAL mode,
//! and is opened by a single owner. Inserts are dedup-aware: re-copying
//! identical content bumps the existing row to the top instead of inserting a
//! duplicate.
//!
//! This crate is deliberately split by responsibility: [`schema`] owns the
//! table definition and migrations, [`paths`] resolves the on-disk location,
//! [`row`] maps between SQL rows and [`Clip`], and this module ([`Store`])
//! only orchestrates queries against an open connection.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use vbuff_types::{Clip, ClipId};

mod error;
mod paths;
mod row;
mod schema;
mod serde_clip;

pub use error::StoreError;
pub use paths::default_db_path;

/// Result type for store operations.
pub type Result<T> = std::result::Result<T, StoreError>;

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
        schema::migrate(&conn)?;
        Ok(Store { conn })
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
            return ClipId::parse(&id_str)
                .map_err(|_| StoreError::Corrupt("bad ulid in db".into()));
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
                row::kind_to_int(clip.meta.kind),
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
        let rows = stmt.query_map(params![limit as i64], row::row_to_clip)?;
        row::collect_clips(rows)
    }

    /// Load the most recent clips (alias used at startup to hydrate the GUI).
    pub fn load_recent(&self, limit: usize) -> Result<Vec<Clip>> {
        self.list(limit)
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
        self.conn
            .execute("DELETE FROM clips WHERE pinned = 0", [])?;
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
    /// Returns the number of clips evicted. This mirrors the policy in
    /// [`vbuff_core::eviction::evict`] but is implemented directly in SQL so a
    /// cap enforcement never has to load the full `Clip` rows (flavor bytes
    /// included) into memory just to compute which ids to drop. The two
    /// implementations are kept honest against each other by
    /// `enforce_cap_matches_pure_eviction_policy` below rather than merged,
    /// since merging would force this hot path back through an in-memory
    /// `Vec<Clip>` fetch.
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

fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
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
            meta: ClipMeta::now(
                ContentKind::Text,
                text.len() as u64,
                Some("test.app".into()),
            ),
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
    fn pin_delete_clear() {
        let store = Store::open_in_memory().unwrap();
        let c = make_clip("findme please");
        let id = store.insert(&c).unwrap();
        store.insert(&make_clip("unrelated")).unwrap();

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

    /// Guards against `enforce_cap`'s hand-written SQL policy silently
    /// drifting from `vbuff_core::eviction::evict`'s pure-logic policy, since
    /// the two are intentionally kept as separate implementations (SQL avoids
    /// loading full `Clip` rows just to compute a cap) rather than merged.
    ///
    /// Inserts are spaced by a couple of milliseconds on purpose: `updated_at`
    /// is stored at millisecond resolution, and ties within the same
    /// millisecond cannot be broken identically by both sides - SQL has the
    /// `seq` autoincrement column to fall back on, but `Clip` deliberately
    /// does not expose a storage-internal sequence number (it stays
    /// storage-agnostic), so a same-millisecond tie is an accepted limit of
    /// the pure policy, not something this test should chase.
    #[test]
    fn enforce_cap_matches_pure_eviction_policy() {
        use vbuff_core::eviction::{EvictionPolicy, evict};

        for max_history in [0usize, 1, 3, 5, 10] {
            let store = Store::open_in_memory().unwrap();
            let mut inserted = Vec::new();
            for i in 0..10 {
                let clip = make_clip(&format!("clip {i}"));
                store.insert(&clip).unwrap();
                inserted.push(clip);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            // Pin two arbitrary clips so the exempt-from-eviction path is
            // exercised by both implementations too.
            store.set_pinned(inserted[2].id, true).unwrap();
            store.set_pinned(inserted[7].id, true).unwrap();

            let before = store.list(100).unwrap();
            let expected_evicted: std::collections::HashSet<_> =
                evict(&before, &EvictionPolicy { max_history })
                    .into_iter()
                    .collect();

            store.enforce_cap(max_history).unwrap();
            let after: std::collections::HashSet<_> =
                store.list(100).unwrap().into_iter().map(|c| c.id).collect();
            let before_ids: std::collections::HashSet<_> = before.iter().map(|c| c.id).collect();
            let actually_evicted: std::collections::HashSet<_> =
                before_ids.difference(&after).copied().collect();

            assert_eq!(
                actually_evicted, expected_evicted,
                "SQL enforce_cap({max_history}) diverged from the pure eviction policy"
            );
        }
    }
}
