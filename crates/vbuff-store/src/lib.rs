//! SQLite-backed persistence for vbuff's clip history.
//!
//! The hot clip row remains compact, while focused side tables own search
//! facets, embeddings, capture metrics, audits, and CAS reference counts.
//!
//! The database lives at `dirs::data_dir()/vbuff/history.db`, runs in WAL mode,
//! and is opened by a single owner. Inserts are dedup-aware: re-copying
//! identical content bumps the existing row to the top instead of inserting a
//! duplicate.
#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::types::Value;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params, params_from_iter};
use serde::{Deserialize, Serialize};
use vbuff_core::bloom::BloomFilter;
use vbuff_core::capture::CaptureOutcome;
use vbuff_core::facets::extract_facets;
use vbuff_core::fingerprint::{
    EmbeddingProvider, LocalFeatureEmbedding, QuantizedEmbedding, fingerprint_bands,
    hamming_distance, simhash64,
};
use vbuff_types::{
    CaptureGeneration, CaptureLineage, CaptureProvenance, Clip, ClipId, ClipMeta, ContentKind,
    Flavor,
};

mod cas;
mod error;
mod image_fingerprint;
mod migration;
mod search;
mod serde_clip;

pub use error::StoreError;

/// Result type for store operations.
pub type Result<T> = std::result::Result<T, StoreError>;

/// The current schema version, stored in `PRAGMA user_version`.
pub const SCHEMA_VERSION: i64 = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct StoreOpenProfile {
    pub private_path_ms: u64,
    pub migration_preflight_ms: u64,
    pub sqlite_open_ms: u64,
    pub initialization_ms: u64,
    pub kdf_ms: Option<u64>,
    pub total_ms: u64,
    pub encryption_enabled: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FtsHealth {
    pub clip_rows: usize,
    pub prose_rows: usize,
    pub code_rows: usize,
    pub missing_rows: usize,
    pub orphan_rows: usize,
    pub dirty_writes: u64,
}

impl FtsHealth {
    pub const fn is_healthy(self) -> bool {
        self.missing_rows == 0 && self.orphan_rows == 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StoreDoctorReport {
    pub schema_version: i64,
    pub expected_schema_version: i64,
    pub quick_check: String,
    pub foreign_key_violations: usize,
    pub clip_rows: usize,
    pub fts: FtsHealth,
    pub cipher_version: Option<String>,
}

impl StoreDoctorReport {
    pub fn is_healthy(&self) -> bool {
        self.schema_version == self.expected_schema_version
            && self.quick_check == "ok"
            && self.foreign_key_violations == 0
            && self.fts.is_healthy()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreMutation {
    SetPinned { id: ClipId, pinned: bool },
    SetFavorite { id: ClipId, favorite: bool },
    Delete { id: ClipId },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SensitiveClawbackReport {
    pub scanned: usize,
    pub reclassified: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchCursor {
    pub pinned: bool,
    pub updated_at: i64,
    pub seq: i64,
}

#[derive(Clone, Debug)]
pub struct SearchPage {
    pub clips: Vec<Clip>,
    pub next_cursor: Option<SearchCursor>,
}

#[derive(Clone, Debug)]
pub struct SearchSession {
    query: String,
    cursor: Option<SearchCursor>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContentAuditReport {
    pub checked: usize,
    pub repaired: usize,
    pub quarantined: usize,
}

#[derive(Serialize)]
struct QuarantineRecord {
    id: String,
    kind: ContentKind,
    byte_size: u64,
    sensitive: bool,
}

impl SearchSession {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            cursor: None,
        }
    }

    pub fn next_page(&mut self, store: &Store, limit: usize) -> Result<SearchPage> {
        let page = store.search_page(&self.query, self.cursor, limit)?;
        self.cursor = page.next_cursor;
        Ok(page)
    }

    pub fn reset(&mut self, query: impl Into<String>) {
        self.query = query.into();
        self.cursor = None;
    }
}

/// A handle to the clip-history database.
pub struct Store {
    conn: Connection,
    cas: Option<cas::CasStore>,
    dedup_filter: RefCell<BloomFilter>,
    search_planner: RefCell<search::SearchPlanner>,
}

impl Store {
    /// Open (creating if necessary) the store at the default data path:
    /// `<data_dir>/vbuff/history.db`.
    pub fn open_default() -> Result<Self> {
        let path = default_db_path()?;
        Self::open(&path)
    }

    /// Open (creating if necessary) the store at a specific path.
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_profiled(path).map(|(store, _)| store)
    }

    /// Open a store and return content-free cold-start timing telemetry.
    pub fn open_profiled(path: &Path) -> Result<(Self, StoreOpenProfile)> {
        let total_started = Instant::now();
        let path_started = Instant::now();
        prepare_private_database_path(path)?;
        let private_path_ms = elapsed_ms(path_started.elapsed());

        let migration_started = Instant::now();
        let migration = migration::MigrationGuard::prepare(path, SCHEMA_VERSION)?;
        if let Some(guard) = &migration {
            let dry_connection = Connection::open(guard.dry_run_path())?;
            let dry_store = Self::from_connection_with_cas(dry_connection, None)?;
            guard.verify_dry_run(&dry_store.conn)?;
            drop(dry_store);
            guard.finish_dry_run()?;
        }
        let migration_preflight_ms = elapsed_ms(migration_started.elapsed());

        let sqlite_started = Instant::now();
        let conn = Connection::open(path)?;
        let sqlite_open_ms = elapsed_ms(sqlite_started.elapsed());
        harden_file_permissions(path)?;
        let cas_root = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("blobs");
        let initialization_started = Instant::now();
        match Self::from_connection_with_cas(conn, Some(cas::CasStore::new(cas_root)?)) {
            Ok(store) => {
                harden_database_files(path)?;
                if let Some(guard) = &migration
                    && let Err(error) = guard.verify_live(&store.conn)
                {
                    drop(store);
                    guard.rollback()?;
                    return Err(error);
                }
                Ok((
                    store,
                    StoreOpenProfile {
                        private_path_ms,
                        migration_preflight_ms,
                        sqlite_open_ms,
                        initialization_ms: elapsed_ms(initialization_started.elapsed()),
                        kdf_ms: None,
                        total_ms: elapsed_ms(total_started.elapsed()),
                        encryption_enabled: false,
                    },
                ))
            }
            Err(error) => {
                if let Some(guard) = &migration {
                    guard.rollback()?;
                }
                Err(error)
            }
        }
    }

    /// Open an existing database without migrations, backfills, CAS cleanup, or writes.
    pub fn open_read_only_profiled(path: &Path) -> Result<(Self, StoreOpenProfile)> {
        let total_started = Instant::now();
        let sqlite_started = Instant::now();
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let sqlite_open_ms = elapsed_ms(sqlite_started.elapsed());
        let encryption_enabled = conn
            .query_row("PRAGMA cipher_version", [], |row| row.get::<_, String>(0))
            .optional()?
            .is_some();
        let store = Store {
            conn,
            cas: None,
            dedup_filter: RefCell::new(BloomFilter::with_capacity(1, 10)),
            search_planner: RefCell::new(search::SearchPlanner::default()),
        };
        Ok((
            store,
            StoreOpenProfile {
                sqlite_open_ms,
                total_ms: elapsed_ms(total_started.elapsed()),
                encryption_enabled,
                ..StoreOpenProfile::default()
            },
        ))
    }

    /// Open an in-memory store (useful for tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        Self::from_connection_with_cas(conn, None)
    }

    fn from_connection_with_cas(conn: Connection, cas: Option<cas::CasStore>) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "secure_delete", "ON")?;
        let mut store = Store {
            conn,
            cas,
            dedup_filter: RefCell::new(BloomFilter::with_capacity(1, 10)),
            search_planner: RefCell::new(search::SearchPlanner::default()),
        };
        store.migrate()?;
        store.backfill_fingerprints(32)?;
        store.rebuild_dedup_filter()?;
        store.gc_blobs()?;
        Ok(store)
    }

    /// Apply forward-only migrations based on `user_version`.
    fn migrate(&mut self) -> Result<()> {
        let transaction = self.conn.transaction()?;
        Self::apply_migrations(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    fn apply_migrations(conn: &Connection) -> Result<()> {
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 1 {
            conn.execute_batch(
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
                    item_text    TEXT NOT NULL DEFAULT '',-- bounded full-text projection
                    metadata_json TEXT NOT NULL DEFAULT '{}',
                    expires_at   INTEGER,
                    simhash      INTEGER,
                    simhash_b0   INTEGER,
                    simhash_b1   INTEGER,
                    simhash_b2   INTEGER,
                    simhash_b3   INTEGER,
                    dhash        INTEGER,
                    dhash_b0     INTEGER,
                    dhash_b1     INTEGER,
                    dhash_b2     INTEGER,
                    dhash_b3     INTEGER,
                    pinned       INTEGER NOT NULL DEFAULT 0,
                    favorite     INTEGER NOT NULL DEFAULT 0
                );
                CREATE UNIQUE INDEX IF NOT EXISTS idx_clips_hash ON clips(content_hash);
                CREATE INDEX IF NOT EXISTS idx_clips_updated ON clips(updated_at DESC, seq DESC);
                CREATE INDEX IF NOT EXISTS idx_clips_pinned ON clips(updated_at DESC) WHERE pinned = 1;
                "#,
            )?;
        }
        if version == 1 {
            conn.execute(
                "ALTER TABLE clips ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}'",
                [],
            )?;
            conn.execute("ALTER TABLE clips ADD COLUMN expires_at INTEGER", [])?;
        }
        if version == 1 || version == 2 {
            conn.execute_batch(
                r#"
                ALTER TABLE clips ADD COLUMN simhash INTEGER;
                ALTER TABLE clips ADD COLUMN simhash_b0 INTEGER;
                ALTER TABLE clips ADD COLUMN simhash_b1 INTEGER;
                ALTER TABLE clips ADD COLUMN simhash_b2 INTEGER;
                ALTER TABLE clips ADD COLUMN simhash_b3 INTEGER;
                ALTER TABLE clips ADD COLUMN dhash INTEGER;
                ALTER TABLE clips ADD COLUMN dhash_b0 INTEGER;
                ALTER TABLE clips ADD COLUMN dhash_b1 INTEGER;
                ALTER TABLE clips ADD COLUMN dhash_b2 INTEGER;
                ALTER TABLE clips ADD COLUMN dhash_b3 INTEGER;
                ALTER TABLE clips ADD COLUMN item_text TEXT NOT NULL DEFAULT '';
                "#,
            )?;
        }
        if version == 3 {
            conn.execute_batch(
                r#"
                ALTER TABLE blob_refs RENAME TO blob_refs_v3;
                CREATE TABLE blob_refs (
                    hash TEXT NOT NULL,
                    kind INTEGER NOT NULL,
                    byte_size INTEGER NOT NULL,
                    refcount INTEGER NOT NULL CHECK(refcount >= 0),
                    PRIMARY KEY (hash, kind)
                ) WITHOUT ROWID;
                INSERT INTO blob_refs(hash, kind, byte_size, refcount)
                    SELECT json_extract(value, '$.body.Spilled.blob_ref'), c.kind,
                           MAX(json_extract(value, '$.body.Spilled.byte_size')), COUNT(*)
                    FROM clips AS c, json_each(c.flavors)
                    WHERE json_type(value, '$.body.Spilled') = 'object'
                    GROUP BY json_extract(value, '$.body.Spilled.blob_ref'), c.kind;
                DROP TABLE blob_refs_v3;
                "#,
            )?;
        }
        conn.execute_batch(
            r#"
            DROP TRIGGER IF EXISTS clips_blob_ai;
            DROP TRIGGER IF EXISTS clips_blob_ad;
            DROP TRIGGER IF EXISTS clips_blob_au;
            "#,
        )?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS capture_metrics (
                metric TEXT PRIMARY KEY,
                count  INTEGER NOT NULL CHECK(count >= 0)
            );

            CREATE TABLE IF NOT EXISTS clip_facets (
                clip_id TEXT NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
                key     TEXT NOT NULL,
                value   TEXT NOT NULL,
                PRIMARY KEY (clip_id, key, value)
            ) WITHOUT ROWID;
            CREATE INDEX IF NOT EXISTS idx_clip_facets_lookup
                ON clip_facets(key, value, clip_id);

            CREATE TABLE IF NOT EXISTS clip_embeddings (
                clip_id TEXT PRIMARY KEY REFERENCES clips(id) ON DELETE CASCADE,
                dimensions INTEGER NOT NULL,
                scale REAL NOT NULL,
                vector BLOB NOT NULL
            );

            CREATE TABLE IF NOT EXISTS blob_refs (
                hash TEXT NOT NULL,
                kind INTEGER NOT NULL,
                byte_size INTEGER NOT NULL,
                refcount INTEGER NOT NULL CHECK(refcount >= 0),
                PRIMARY KEY (hash, kind)
            ) WITHOUT ROWID;

            CREATE TABLE IF NOT EXISTS maintenance_state (
                key TEXT PRIMARY KEY,
                value INTEGER NOT NULL
            );
            INSERT OR IGNORE INTO maintenance_state(key, value) VALUES ('fts_dirty', 0);
            INSERT OR IGNORE INTO maintenance_state(key, value) VALUES ('secret_scan_cursor', 0);

            CREATE TABLE IF NOT EXISTS content_audit (
                clip_id TEXT PRIMARY KEY REFERENCES clips(id) ON DELETE CASCADE,
                checked_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS quarantined_clips (
                id TEXT PRIMARY KEY,
                quarantined_at INTEGER NOT NULL,
                reason TEXT NOT NULL,
                row_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_clips_simhash ON clips(simhash);
            CREATE INDEX IF NOT EXISTS idx_clips_simhash_b0 ON clips(simhash_b0);
            CREATE INDEX IF NOT EXISTS idx_clips_simhash_b1 ON clips(simhash_b1);
            CREATE INDEX IF NOT EXISTS idx_clips_simhash_b2 ON clips(simhash_b2);
            CREATE INDEX IF NOT EXISTS idx_clips_simhash_b3 ON clips(simhash_b3);
            CREATE INDEX IF NOT EXISTS idx_clips_dhash ON clips(dhash);
            CREATE INDEX IF NOT EXISTS idx_clips_dhash_b0 ON clips(dhash_b0);
            CREATE INDEX IF NOT EXISTS idx_clips_dhash_b1 ON clips(dhash_b1);
            CREATE INDEX IF NOT EXISTS idx_clips_dhash_b2 ON clips(dhash_b2);
            CREATE INDEX IF NOT EXISTS idx_clips_dhash_b3 ON clips(dhash_b3);

            CREATE VIRTUAL TABLE IF NOT EXISTS clip_fts_prose
                USING fts5(item_text, tokenize='unicode61 remove_diacritics 2');
            CREATE VIRTUAL TABLE IF NOT EXISTS clip_fts_code
                USING fts5(item_text, tokenize='trigram');

            CREATE TRIGGER IF NOT EXISTS clips_fts_ai AFTER INSERT ON clips BEGIN
                INSERT INTO clip_fts_prose(rowid, item_text) VALUES (new.seq, new.item_text);
                INSERT INTO clip_fts_code(rowid, item_text)
                    SELECT new.seq, new.item_text WHERE new.kind = 7;
                INSERT INTO maintenance_state(key, value) VALUES ('fts_dirty', 1)
                    ON CONFLICT(key) DO UPDATE SET value = value + 1;
            END;
            CREATE TRIGGER IF NOT EXISTS clips_fts_ad AFTER DELETE ON clips BEGIN
                DELETE FROM clip_fts_prose WHERE rowid = old.seq;
                DELETE FROM clip_fts_code WHERE rowid = old.seq;
                INSERT INTO maintenance_state(key, value) VALUES ('fts_dirty', 1)
                    ON CONFLICT(key) DO UPDATE SET value = value + 1;
            END;
            CREATE TRIGGER IF NOT EXISTS clips_fts_au
                AFTER UPDATE OF seq, item_text, kind ON clips BEGIN
                DELETE FROM clip_fts_prose WHERE rowid = old.seq;
                DELETE FROM clip_fts_code WHERE rowid = old.seq;
                INSERT INTO clip_fts_prose(rowid, item_text) VALUES (new.seq, new.item_text);
                INSERT INTO clip_fts_code(rowid, item_text)
                    SELECT new.seq, new.item_text WHERE new.kind = 7;
                INSERT INTO maintenance_state(key, value) VALUES ('fts_dirty', 1)
                    ON CONFLICT(key) DO UPDATE SET value = value + 1;
            END;

            CREATE TRIGGER IF NOT EXISTS clips_blob_ai AFTER INSERT ON clips BEGIN
                INSERT INTO blob_refs(hash, kind, byte_size, refcount)
                SELECT json_extract(value, '$.body.Spilled.blob_ref'), new.kind,
                       MAX(json_extract(value, '$.body.Spilled.byte_size')), COUNT(*)
                FROM json_each(new.flavors)
                WHERE json_type(value, '$.body.Spilled') = 'object'
                GROUP BY json_extract(value, '$.body.Spilled.blob_ref')
                ON CONFLICT(hash, kind) DO UPDATE
                    SET refcount = refcount + excluded.refcount;
            END;
            CREATE TRIGGER IF NOT EXISTS clips_blob_ad AFTER DELETE ON clips BEGIN
                UPDATE blob_refs
                SET refcount = refcount - (
                    SELECT COUNT(*) FROM json_each(old.flavors)
                    WHERE json_type(value, '$.body.Spilled') = 'object'
                      AND json_extract(value, '$.body.Spilled.blob_ref') = blob_refs.hash
                )
                WHERE kind = old.kind AND hash IN (
                    SELECT json_extract(value, '$.body.Spilled.blob_ref')
                    FROM json_each(old.flavors)
                    WHERE json_type(value, '$.body.Spilled') = 'object'
                );
            END;
            CREATE TRIGGER IF NOT EXISTS clips_blob_au AFTER UPDATE OF flavors ON clips BEGIN
                UPDATE blob_refs
                SET refcount = refcount - (
                    SELECT COUNT(*) FROM json_each(old.flavors)
                    WHERE json_type(value, '$.body.Spilled') = 'object'
                      AND json_extract(value, '$.body.Spilled.blob_ref') = blob_refs.hash
                )
                WHERE kind = old.kind AND hash IN (
                    SELECT json_extract(value, '$.body.Spilled.blob_ref')
                    FROM json_each(old.flavors)
                    WHERE json_type(value, '$.body.Spilled') = 'object'
                );
                INSERT INTO blob_refs(hash, kind, byte_size, refcount)
                SELECT json_extract(value, '$.body.Spilled.blob_ref'), new.kind,
                       MAX(json_extract(value, '$.body.Spilled.byte_size')), COUNT(*)
                FROM json_each(new.flavors)
                WHERE json_type(value, '$.body.Spilled') = 'object'
                GROUP BY json_extract(value, '$.body.Spilled.blob_ref')
                ON CONFLICT(hash, kind) DO UPDATE
                    SET refcount = refcount + excluded.refcount;
            END;
            "#,
        )?;
        conn.execute(
            "UPDATE clips SET item_text = preview WHERE item_text = ''",
            [],
        )?;
        let fts_in_sync: bool = conn.query_row(
            r#"
            SELECT (SELECT COUNT(*) FROM clip_fts_prose) = (SELECT COUNT(*) FROM clips)
               AND (SELECT COUNT(*) FROM clip_fts_code) =
                   (SELECT COUNT(*) FROM clips WHERE kind = 7)
            "#,
            [],
            |row| row.get(0),
        )?;
        if !fts_in_sync {
            conn.execute_batch(
                r#"
                DELETE FROM clip_fts_prose;
                DELETE FROM clip_fts_code;
                INSERT INTO clip_fts_prose(rowid, item_text)
                    SELECT seq, item_text FROM clips;
                INSERT INTO clip_fts_code(rowid, item_text)
                    SELECT seq, item_text FROM clips WHERE kind = 7;
                "#,
            )?;
        }
        conn.execute_batch(
            r#"
            INSERT INTO clip_fts_prose(clip_fts_prose, rank) VALUES('automerge', 4);
            INSERT INTO clip_fts_code(clip_fts_code, rank) VALUES('automerge', 4);
            "#,
        )?;
        if version < SCHEMA_VERSION {
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(())
    }

    fn rebuild_dedup_filter(&self) -> Result<()> {
        let count: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM clips", [], |row| row.get::<_, i64>(0))?
            as usize;
        let mut filter = BloomFilter::with_capacity(count.max(1_024), 10);
        let mut statement = self.conn.prepare("SELECT content_hash FROM clips")?;
        let hashes = statement.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
        for hash in hashes {
            filter.insert(&hash?);
        }
        *self.dedup_filter.borrow_mut() = filter;
        Ok(())
    }

    /// Backfill a bounded number of fingerprints for rows from older schemas.
    pub fn backfill_fingerprints(&self, limit: usize) -> Result<usize> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT id, kind, item_text, flavors
            FROM clips
            WHERE (simhash IS NULL AND kind != 3 AND item_text != '')
               OR (dhash IS NULL AND kind = 3)
            LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map(params![limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let pending = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        let transaction = self.conn.unchecked_transaction()?;
        let count = pending.len();
        for (id, kind, item_text, flavors_json) in pending {
            let simhash = (kind != kind_to_int(ContentKind::Image)).then(|| simhash64(&item_text));
            let dhash = if kind == kind_to_int(ContentKind::Image) {
                let flavors = serde_clip::flavors_from_json(&flavors_json)?;
                let byte_size = flavors.iter().map(|flavor| flavor.body.byte_size()).sum();
                let clip = Clip {
                    id: ClipId::parse(&id)
                        .map_err(|_| StoreError::Corrupt("bad ulid in db".into()))?,
                    flavors,
                    content_hash: [0; 32],
                    meta: ClipMeta::now(ContentKind::Image, byte_size, None),
                    pinned: false,
                    favorite: false,
                };
                image_fingerprint::clip_dhash(&clip)
            } else {
                None
            };
            let simhash_bands = simhash.map(fingerprint_bands);
            let dhash_bands = dhash.map(fingerprint_bands);
            transaction.execute(
                r#"
                UPDATE clips SET
                    simhash = ?1, simhash_b0 = ?2, simhash_b1 = ?3,
                    simhash_b2 = ?4, simhash_b3 = ?5,
                    dhash = ?6, dhash_b0 = ?7, dhash_b1 = ?8,
                    dhash_b2 = ?9, dhash_b3 = ?10
                WHERE id = ?11
                "#,
                params![
                    simhash.map(|value| value as i64),
                    simhash_bands.map(|bands| i64::from(bands[0])),
                    simhash_bands.map(|bands| i64::from(bands[1])),
                    simhash_bands.map(|bands| i64::from(bands[2])),
                    simhash_bands.map(|bands| i64::from(bands[3])),
                    dhash.map(|value| value as i64),
                    dhash_bands.map(|bands| i64::from(bands[0])),
                    dhash_bands.map(|bands| i64::from(bands[1])),
                    dhash_bands.map(|bands| i64::from(bands[2])),
                    dhash_bands.map(|bands| i64::from(bands[3])),
                    id,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(count)
    }

    /// Insert a clip, deduplicating by content hash.
    ///
    /// If a clip with the same `content_hash` already exists, its `updated_at`
    /// is bumped to now (moving it to the top) and its existing [`ClipId`] is
    /// returned. Otherwise the new clip is inserted and its id returned.
    pub fn insert(&self, clip: &Clip) -> Result<ClipId> {
        self.purge_expired()?;
        let now = now_millis();
        let metadata_json = serde_json::to_string(&StoredMetadata::from(&clip.meta))?;
        let expires_at = clip.meta.expires_at.map(|value| value.timestamp_millis());
        let preview = clip.preview(512);
        let item_text = searchable_projection(clip, 1_048_576);
        let simhash = clip.primary_text().map(simhash64);
        let simhash_bands = simhash.map(fingerprint_bands);
        let dhash = image_fingerprint::clip_dhash(clip);
        let dhash_bands = dhash.map(fingerprint_bands);
        let facets = clip
            .primary_text()
            .map(|text| extract_facets(text, clip.meta.kind, clip.meta.sensitive))
            .unwrap_or_default();
        let might_exist = self
            .dedup_filter
            .borrow()
            .might_contain(clip.content_hash.as_slice());
        let transaction = self.conn.unchecked_transaction()?;

        // Dedup: does this content already exist?
        let existing: Option<String> = if might_exist {
            transaction
                .query_row(
                    "SELECT id FROM clips WHERE content_hash = ?1",
                    params![clip.content_hash.as_slice()],
                    |row| row.get(0),
                )
                .optional()?
        } else {
            None
        };

        if let Some(id_str) = existing {
            // Bump both updated_at and seq so the deduped clip floats to the top
            // even when several inserts share the same millisecond.
            transaction.execute(
                r#"
                UPDATE clips
                SET updated_at = ?1,
                    seq = (SELECT COALESCE(MAX(seq), 0) + 1 FROM clips),
                    metadata_json = ?2,
                    expires_at = ?3,
                    source_app = ?4
                WHERE id = ?5
                "#,
                params![now, metadata_json, expires_at, clip.meta.source_app, id_str],
            )?;
            transaction.commit()?;
            return ClipId::parse(&id_str)
                .map_err(|_| StoreError::Corrupt("bad ulid in db".into()));
        }

        let mut stored_flavors = clip.flavors.clone();
        if !clip.meta.sensitive
            && let Some(cas) = &self.cas
        {
            cas.spill_flavors(&mut stored_flavors, clip.meta.kind)?;
        }
        let flavors_json = serde_clip::flavors_to_json(&stored_flavors)?;
        let created = clip.meta.created_at.timestamp_millis();

        transaction.execute(
            r#"
            INSERT INTO clips
                (id, content_hash, flavors, kind, created_at, updated_at,
                 byte_size, source_app, preview, item_text, metadata_json, expires_at,
                 simhash, simhash_b0, simhash_b1, simhash_b2, simhash_b3,
                 dhash, dhash_b0, dhash_b1, dhash_b2, dhash_b3, pinned, favorite)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                    ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)
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
                item_text,
                metadata_json,
                expires_at,
                simhash.map(|value| value as i64),
                simhash_bands.map(|bands| i64::from(bands[0])),
                simhash_bands.map(|bands| i64::from(bands[1])),
                simhash_bands.map(|bands| i64::from(bands[2])),
                simhash_bands.map(|bands| i64::from(bands[3])),
                dhash.map(|value| value as i64),
                dhash_bands.map(|bands| i64::from(bands[0])),
                dhash_bands.map(|bands| i64::from(bands[1])),
                dhash_bands.map(|bands| i64::from(bands[2])),
                dhash_bands.map(|bands| i64::from(bands[3])),
                clip.pinned as i64,
                clip.favorite as i64,
            ],
        )?;
        for facet in facets {
            transaction.execute(
                "INSERT OR IGNORE INTO clip_facets(clip_id, key, value) VALUES (?1, ?2, ?3)",
                params![clip.id.to_string_repr(), facet.key, facet.value],
            )?;
        }
        transaction.commit()?;
        self.dedup_filter
            .borrow_mut()
            .insert(clip.content_hash.as_slice());
        Ok(clip.id)
    }

    /// List the most recent clips (pinned first, then by recency), up to `limit`.
    pub fn list(&self, limit: usize) -> Result<Vec<Clip>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, content_hash, flavors, kind, created_at, updated_at,
                   byte_size, source_app, metadata_json, pinned, favorite
            FROM clips
            WHERE expires_at IS NULL OR expires_at > ?1
            ORDER BY pinned DESC, updated_at DESC, seq DESC
            LIMIT ?2
            "#,
        )?;
        let rows = stmt.query_map(params![now_millis(), limit as i64], row_to_clip)?;
        let mut clips = collect_clips(rows)?;
        self.hydrate_clips(&mut clips)?;
        Ok(clips)
    }

    /// Load the most recent clips (alias used at startup to hydrate the GUI).
    pub fn load_recent(&self, limit: usize) -> Result<Vec<Clip>> {
        self.list(limit)
    }

    /// Search with an adaptive LIKE/FTS5 tier and structured facet filters.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Clip>> {
        Ok(self.search_page(query, None, limit)?.clips)
    }

    pub fn search_page(
        &self,
        query: &str,
        cursor: Option<SearchCursor>,
        limit: usize,
    ) -> Result<SearchPage> {
        if limit == 0 {
            return Ok(SearchPage {
                clips: Vec::new(),
                next_cursor: None,
            });
        }
        let parsed = search::parse_query(query);
        let row_count = self.count()?;
        let use_fts = !parsed.text.is_empty()
            && self
                .search_planner
                .borrow()
                .use_fts(row_count, &parsed.text);
        let started = Instant::now();
        let mut sql = String::from(
            r#"
            SELECT c.id, c.content_hash, c.flavors, c.kind, c.created_at, c.updated_at,
                   c.byte_size, c.source_app, c.metadata_json, c.pinned, c.favorite, c.seq
            FROM clips c
            WHERE (c.expires_at IS NULL OR c.expires_at > ?)
            "#,
        );
        let mut values = vec![Value::Integer(now_millis())];

        if !parsed.text.is_empty() {
            if use_fts {
                let literal = search::fts_literal(&parsed.text);
                sql.push_str(
                    r#"
                    AND c.seq IN (
                        SELECT rowid FROM clip_fts_prose WHERE clip_fts_prose MATCH ?
                        UNION
                        SELECT rowid FROM clip_fts_code WHERE clip_fts_code MATCH ?
                    )
                    "#,
                );
                values.push(Value::Text(literal.clone()));
                values.push(Value::Text(literal));
            } else {
                let pattern = format!("%{}%", escape_like(&parsed.text));
                sql.push_str(
                    " AND (c.item_text LIKE ? ESCAPE '\\' OR c.source_app LIKE ? ESCAPE '\\')",
                );
                values.push(Value::Text(pattern.clone()));
                values.push(Value::Text(pattern));
            }
        }
        for (key, value) in parsed.facets {
            sql.push_str(
                r#"
                AND EXISTS (
                    SELECT 1 FROM clip_facets f
                    WHERE f.clip_id = c.id AND f.key = ? AND f.value = ?
                )
                "#,
            );
            values.push(Value::Text(key));
            values.push(Value::Text(value));
        }
        if let Some(cursor) = cursor {
            sql.push_str(
                " AND (c.pinned < ? OR (c.pinned = ? AND (c.updated_at < ? OR (c.updated_at = ? AND c.seq < ?))))",
            );
            values.push(Value::Integer(cursor.pinned as i64));
            values.push(Value::Integer(cursor.pinned as i64));
            values.push(Value::Integer(cursor.updated_at));
            values.push(Value::Integer(cursor.updated_at));
            values.push(Value::Integer(cursor.seq));
        }
        sql.push_str(" ORDER BY c.pinned DESC, c.updated_at DESC, c.seq DESC LIMIT ?");
        values.push(Value::Integer(limit as i64));

        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(values.iter()), |row| {
            Ok((
                row_to_clip(row)?,
                SearchCursor {
                    pinned: row.get::<_, i64>(9)? != 0,
                    updated_at: row.get(5)?,
                    seq: row.get(11)?,
                },
            ))
        })?;
        let mut clips = Vec::new();
        let mut last_cursor = None;
        for row in rows {
            let (raw, row_cursor) = row?;
            clips.push(raw_to_clip(raw)?);
            last_cursor = Some(row_cursor);
        }
        self.hydrate_clips(&mut clips)?;
        if !use_fts && !parsed.text.is_empty() {
            self.search_planner
                .borrow_mut()
                .record_like(started.elapsed());
        }
        let next_cursor = (clips.len() == limit).then_some(last_cursor).flatten();
        Ok(SearchPage { clips, next_cursor })
    }

    pub fn find_near_text(&self, text: &str, max_distance: u32, limit: usize) -> Result<Vec<Clip>> {
        self.find_near_fingerprint("simhash", simhash64(text), max_distance, limit)
    }

    pub fn find_near_image(
        &self,
        fingerprint: u64,
        max_distance: u32,
        limit: usize,
    ) -> Result<Vec<Clip>> {
        self.find_near_fingerprint("dhash", fingerprint, max_distance, limit)
    }

    fn find_near_fingerprint(
        &self,
        column: &str,
        fingerprint: u64,
        max_distance: u32,
        limit: usize,
    ) -> Result<Vec<Clip>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let bands = fingerprint_bands(fingerprint);
        // Four equal 16-bit bands are a complete candidate filter only for
        // distances below four. At larger distances, scan the fingerprint
        // column so one changed bit per band cannot become a false negative.
        let band_filter = if max_distance < 4 {
            format!(
                "AND ({column}_b0 = ? OR {column}_b1 = ? OR {column}_b2 = ? OR {column}_b3 = ?)"
            )
        } else {
            String::new()
        };
        let sql = format!(
            r#"
            SELECT id, content_hash, flavors, kind, created_at, updated_at,
                   byte_size, source_app, metadata_json, pinned, favorite, {column}
            FROM clips
            WHERE {column} IS NOT NULL
              {band_filter}
            ORDER BY updated_at DESC, seq DESC
            "#
        );
        let mut statement = self.conn.prepare(&sql)?;
        let values = if max_distance < 4 {
            bands
                .into_iter()
                .map(|band| Value::Integer(i64::from(band)))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let rows = statement.query_map(params_from_iter(values.iter()), |row| {
            Ok((row_to_clip(row)?, row.get::<_, i64>(11)? as u64))
        })?;
        let mut matches = Vec::new();
        for row in rows {
            let (raw, candidate) = row?;
            if hamming_distance(fingerprint, candidate) <= max_distance {
                let mut clip = raw_to_clip(raw)?;
                self.hydrate_clip(&mut clip)?;
                matches.push(clip);
                if matches.len() == limit {
                    break;
                }
            }
        }
        Ok(matches)
    }

    fn hydrate_clips(&self, clips: &mut [Clip]) -> Result<()> {
        for clip in clips {
            self.hydrate_clip(clip)?;
        }
        Ok(())
    }

    fn hydrate_clip(&self, clip: &mut Clip) -> Result<()> {
        if let Some(cas) = &self.cas {
            cas.hydrate_flavors(&mut clip.flavors, clip.meta.kind)?;
        }
        Ok(())
    }

    /// Lazily build compact local feature vectors during an idle window.
    pub fn backfill_embeddings(&self, limit: usize) -> Result<usize> {
        let provider = LocalFeatureEmbedding;
        let mut statement = self.conn.prepare(
            r#"
            SELECT c.id, c.item_text
            FROM clips c
            LEFT JOIN clip_embeddings e ON e.clip_id = c.id
            WHERE e.clip_id IS NULL
              AND c.item_text != ''
              AND COALESCE(json_extract(c.metadata_json, '$.sensitive'), 0) = 0
            LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map(params![limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let pending = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        let transaction = self.conn.unchecked_transaction()?;
        for (id, text) in &pending {
            let embedding = provider.embed(text);
            let bytes = embedding
                .values
                .iter()
                .map(|value| *value as u8)
                .collect::<Vec<_>>();
            transaction.execute(
                r#"
                INSERT OR REPLACE INTO clip_embeddings(clip_id, dimensions, scale, vector)
                VALUES (?1, ?2, ?3, ?4)
                "#,
                params![id, embedding.values.len() as i64, embedding.scale, bytes],
            )?;
        }
        transaction.commit()?;
        Ok(pending.len())
    }

    /// Hybrid local similarity search: narrow lexically, then rerank at most
    /// 512 candidates with compact feature vectors.
    pub fn local_similarity_search(&self, query: &str, limit: usize) -> Result<Vec<Clip>> {
        let output_limit = limit.min(512);
        if output_limit == 0 {
            return Ok(Vec::new());
        }
        let provider = LocalFeatureEmbedding;
        let query_embedding = provider.embed(query);
        let candidate_limit = output_limit.saturating_mul(8).min(512).max(output_limit);
        let mut candidates = self.search(query, candidate_limit)?;
        if candidates.is_empty() {
            candidates = self.list(candidate_limit)?;
        }
        let mut statement = self
            .conn
            .prepare("SELECT dimensions, scale, vector FROM clip_embeddings WHERE clip_id = ?1")?;
        let mut scored = candidates
            .into_iter()
            .map(|clip| {
                let embedding = statement
                    .query_row(params![clip.id.to_string_repr()], |row| {
                        let dimensions = row.get::<_, i64>(0)? as usize;
                        let scale = row.get::<_, f32>(1)?;
                        let bytes = row.get::<_, Vec<u8>>(2)?;
                        if dimensions != bytes.len() {
                            return Err(rusqlite::Error::InvalidQuery);
                        }
                        Ok(QuantizedEmbedding {
                            scale,
                            values: bytes.into_iter().map(|byte| byte as i8).collect(),
                        })
                    })
                    .optional()?;
                let score = embedding
                    .as_ref()
                    .and_then(|embedding| query_embedding.cosine_similarity(embedding))
                    .unwrap_or(f32::NEG_INFINITY);
                Ok::<_, StoreError>((clip, score))
            })
            .collect::<Result<Vec<_>>>()?;
        scored.sort_by(|left, right| right.1.total_cmp(&left.1));
        scored.truncate(output_limit);
        Ok(scored.into_iter().map(|(clip, _)| clip).collect())
    }

    /// Run bounded FTS maintenance only after meaningful write churn.
    pub fn maintain_search_index(&self, dirty_threshold: u64) -> Result<bool> {
        let dirty_threshold = dirty_threshold.max(1);
        let dirty: i64 = self.conn.query_row(
            "SELECT value FROM maintenance_state WHERE key = 'fts_dirty'",
            [],
            |row| row.get(0),
        )?;
        if dirty < dirty_threshold.min(i64::MAX as u64) as i64 {
            return Ok(false);
        }
        for table in ["clip_fts_prose", "clip_fts_code"] {
            self.conn.execute(
                &format!("INSERT INTO {table}({table}, rank) VALUES('merge', ?1)"),
                [32_i64],
            )?;
        }
        if dirty >= dirty_threshold.saturating_mul(4).min(i64::MAX as u64) as i64 {
            self.conn.execute_batch(
                r#"
                INSERT INTO clip_fts_prose(clip_fts_prose) VALUES('optimize');
                INSERT INTO clip_fts_code(clip_fts_code) VALUES('optimize');
                "#,
            )?;
        }
        self.conn.execute_batch(
            r#"
            INSERT INTO clip_fts_prose(clip_fts_prose) VALUES('integrity-check');
            INSERT INTO clip_fts_code(clip_fts_code) VALUES('integrity-check');
            UPDATE maintenance_state SET value = 0 WHERE key = 'fts_dirty';
            "#,
        )?;
        Ok(true)
    }

    /// Compare FTS row identities with their source rows and expose write churn.
    pub fn fts_health(&self) -> Result<FtsHealth> {
        let clip_rows = query_count(&self.conn, "SELECT COUNT(*) FROM clips")?;
        let prose_rows = query_count(&self.conn, "SELECT COUNT(*) FROM clip_fts_prose")?;
        let code_rows = query_count(&self.conn, "SELECT COUNT(*) FROM clip_fts_code")?;
        let missing_prose = query_count(
            &self.conn,
            r#"
            SELECT COUNT(*) FROM clips AS c
            LEFT JOIN clip_fts_prose AS f ON f.rowid = c.seq
            WHERE f.rowid IS NULL
            "#,
        )?;
        let missing_code = query_count(
            &self.conn,
            r#"
            SELECT COUNT(*) FROM clips AS c
            LEFT JOIN clip_fts_code AS f ON f.rowid = c.seq
            WHERE c.kind = 7 AND f.rowid IS NULL
            "#,
        )?;
        let orphan_prose = query_count(
            &self.conn,
            r#"
            SELECT COUNT(*) FROM clip_fts_prose AS f
            LEFT JOIN clips AS c ON c.seq = f.rowid
            WHERE c.seq IS NULL
            "#,
        )?;
        let orphan_code = query_count(
            &self.conn,
            r#"
            SELECT COUNT(*) FROM clip_fts_code AS f
            LEFT JOIN clips AS c ON c.seq = f.rowid AND c.kind = 7
            WHERE c.seq IS NULL
            "#,
        )?;
        let dirty: i64 = self.conn.query_row(
            "SELECT value FROM maintenance_state WHERE key = 'fts_dirty'",
            [],
            |row| row.get(0),
        )?;
        Ok(FtsHealth {
            clip_rows,
            prose_rows,
            code_rows,
            missing_rows: missing_prose.saturating_add(missing_code),
            orphan_rows: orphan_prose.saturating_add(orphan_code),
            dirty_writes: dirty.max(0) as u64,
        })
    }

    /// Run read-only checks suitable for `vbuff doctor --json`.
    pub fn doctor(&self) -> Result<StoreDoctorReport> {
        let schema_version = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        let quick_check = self
            .conn
            .query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        let mut foreign_keys = self.conn.prepare("PRAGMA foreign_key_check")?;
        let mut foreign_key_rows = foreign_keys.query([])?;
        let mut foreign_key_violations = 0_usize;
        while foreign_key_rows.next()?.is_some() {
            foreign_key_violations = foreign_key_violations.saturating_add(1);
        }
        let cipher_version = self
            .conn
            .query_row("PRAGMA cipher_version", [], |row| row.get(0))
            .optional()?;
        Ok(StoreDoctorReport {
            schema_version,
            expected_schema_version: SCHEMA_VERSION,
            quick_check,
            foreign_key_violations,
            clip_rows: query_count(&self.conn, "SELECT COUNT(*) FROM clips")?,
            fts: self.fts_health()?,
            cipher_version,
        })
    }

    /// Reclassify a bounded set of historical structural secrets.
    pub fn clawback_sensitive(
        &self,
        limit: usize,
        ttl: Duration,
    ) -> Result<SensitiveClawbackReport> {
        let limit = limit.min(i64::MAX as usize);
        if limit == 0 {
            return Ok(SensitiveClawbackReport::default());
        }
        let cursor: i64 = self.conn.query_row(
            "SELECT value FROM maintenance_state WHERE key = 'secret_scan_cursor'",
            [],
            |row| row.get(0),
        )?;
        let mut statement = self.conn.prepare(
            r#"
            SELECT seq, id, metadata_json, item_text FROM clips
            WHERE COALESCE(json_extract(metadata_json, '$.sensitive'), 0) = 0
              AND item_text <> ''
              AND seq > ?1
            ORDER BY seq ASC
            LIMIT ?2
            "#,
        )?;
        let candidates = statement
            .query_map(params![cursor, limit as i64], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);

        let mut report = SensitiveClawbackReport {
            scanned: candidates.len(),
            reclassified: 0,
        };
        let expires_at = chrono::Utc::now()
            + chrono::Duration::from_std(ttl.max(Duration::from_secs(1)))
                .unwrap_or_else(|_| chrono::Duration::days(1));
        let transaction = self.conn.unchecked_transaction()?;
        let next_cursor = if candidates.len() < limit {
            0
        } else {
            candidates.last().map_or(0, |candidate| candidate.0)
        };
        for (_, id, metadata_json, item_text) in candidates {
            let detected = vbuff_core::secret::detect_secrets(&item_text)
                .iter()
                .any(|finding| finding.confidence >= 0.9);
            if !detected {
                continue;
            }
            let mut metadata: StoredMetadata = serde_json::from_str(&metadata_json)?;
            metadata.sensitive = true;
            metadata.sync_eligible = Some(false);
            metadata.expires_at = Some(expires_at);
            transaction.execute(
                r#"
                UPDATE clips SET metadata_json = ?1, expires_at = ?2,
                    preview = '[sensitive]', item_text = ''
                WHERE id = ?3
                "#,
                params![
                    serde_json::to_string(&metadata)?,
                    expires_at.timestamp_millis(),
                    id
                ],
            )?;
            transaction.execute("DELETE FROM clip_facets WHERE clip_id = ?1", [&id])?;
            transaction.execute("DELETE FROM clip_embeddings WHERE clip_id = ?1", [&id])?;
            report.reclassified += 1;
        }
        transaction.execute(
            "UPDATE maintenance_state SET value = ?1 WHERE key = 'secret_scan_cursor'",
            [next_cursor],
        )?;
        transaction.commit()?;
        if report.reclassified > 0 {
            self.conn.execute_batch(
                r#"
                INSERT INTO clip_fts_prose(clip_fts_prose) VALUES('optimize');
                INSERT INTO clip_fts_code(clip_fts_code) VALUES('optimize');
                "#,
            )?;
            self.scrub_deleted_pages()?;
        }
        Ok(report)
    }

    /// Remove CAS files whose transactional refcount reached zero or which a
    /// crash stranded before the corresponding database commit.
    pub fn gc_blobs(&self) -> Result<usize> {
        let Some(cas) = &self.cas else {
            return Ok(0);
        };
        let mut statement = self
            .conn
            .prepare("SELECT hash, kind FROM blob_refs WHERE refcount = 0")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let dead = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        for (blob_ref, kind) in &dead {
            cas.remove(kind_from_int(*kind), blob_ref)?;
        }
        self.conn
            .execute("DELETE FROM blob_refs WHERE refcount = 0", [])?;
        let mut statement = self
            .conn
            .prepare("SELECT hash, kind FROM blob_refs WHERE refcount > 0")?;
        let rows = statement.query_map([], |row| {
            Ok((
                kind_from_int(row.get::<_, i64>(1)?),
                row.get::<_, String>(0)?,
            ))
        })?;
        let live = rows.collect::<rusqlite::Result<HashSet<_>>>()?;
        drop(statement);
        let orphans = cas.remove_orphans(&live)?;
        Ok(dead.len() + orphans)
    }

    /// Recompute a rolling sample and repair or quarantine hash mismatches.
    pub fn audit_content_hashes(&self, limit: usize) -> Result<ContentAuditReport> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT c.id, c.content_hash, c.flavors, c.kind, c.created_at, c.updated_at,
                   c.byte_size, c.source_app, c.metadata_json, c.pinned, c.favorite
            FROM clips c
            LEFT JOIN content_audit a ON a.clip_id = c.id
            ORDER BY COALESCE(a.checked_at, 0) ASC, c.seq ASC
            LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map(params![limit as i64], row_to_clip)?;
        let mut candidates = collect_clips(rows)?;
        drop(statement);
        self.hydrate_clips(&mut candidates)?;

        let mut report = ContentAuditReport::default();
        let transaction = self.conn.unchecked_transaction()?;
        for clip in candidates {
            report.checked += 1;
            let actual = vbuff_core::content_hash_from_flavors(&clip.flavors);
            if actual == clip.content_hash {
                transaction.execute(
                    r#"
                    INSERT INTO content_audit(clip_id, checked_at) VALUES (?1, ?2)
                    ON CONFLICT(clip_id) DO UPDATE SET checked_at = excluded.checked_at
                    "#,
                    params![clip.id.to_string_repr(), now_millis()],
                )?;
                continue;
            }

            let conflict: Option<String> = transaction
                .query_row(
                    "SELECT id FROM clips WHERE content_hash = ?1 AND id != ?2",
                    params![actual.as_slice(), clip.id.to_string_repr()],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(conflict_id) = conflict {
                let row_json = serde_json::to_string(&QuarantineRecord {
                    id: clip.id.to_string_repr(),
                    kind: clip.meta.kind,
                    byte_size: clip.meta.byte_size,
                    sensitive: clip.meta.sensitive,
                })?;
                transaction.execute(
                    r#"
                    INSERT OR REPLACE INTO quarantined_clips
                        (id, quarantined_at, reason, row_json)
                    VALUES (?1, ?2, ?3, ?4)
                    "#,
                    params![
                        clip.id.to_string_repr(),
                        now_millis(),
                        format!("content hash conflicts with {conflict_id}"),
                        row_json,
                    ],
                )?;
                transaction.execute(
                    "DELETE FROM clips WHERE id = ?1",
                    params![clip.id.to_string_repr()],
                )?;
                report.quarantined += 1;
            } else {
                transaction.execute(
                    "UPDATE clips SET content_hash = ?1 WHERE id = ?2",
                    params![actual.as_slice(), clip.id.to_string_repr()],
                )?;
                transaction.execute(
                    r#"
                    INSERT INTO content_audit(clip_id, checked_at) VALUES (?1, ?2)
                    ON CONFLICT(clip_id) DO UPDATE SET checked_at = excluded.checked_at
                    "#,
                    params![clip.id.to_string_repr(), now_millis()],
                )?;
                self.dedup_filter.borrow_mut().insert(actual.as_slice());
                report.repaired += 1;
            }
        }
        transaction.commit()?;
        if report.quarantined > 0 {
            self.scrub_deleted_pages()?;
        }
        Ok(report)
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

    /// Apply all mutations in one SQLite transaction or roll every one back.
    pub fn apply_batch(&self, mutations: &[StoreMutation]) -> Result<usize> {
        let transaction = self.conn.unchecked_transaction()?;
        let mut deleted = false;
        for mutation in mutations {
            let (id, changed) = match *mutation {
                StoreMutation::SetPinned { id, pinned } => (
                    id,
                    transaction.execute(
                        "UPDATE clips SET pinned = ?1 WHERE id = ?2",
                        params![pinned as i64, id.to_string_repr()],
                    )?,
                ),
                StoreMutation::SetFavorite { id, favorite } => (
                    id,
                    transaction.execute(
                        "UPDATE clips SET favorite = ?1 WHERE id = ?2",
                        params![favorite as i64, id.to_string_repr()],
                    )?,
                ),
                StoreMutation::Delete { id } => {
                    deleted = true;
                    (
                        id,
                        transaction
                            .execute("DELETE FROM clips WHERE id = ?1", [id.to_string_repr()])?,
                    )
                }
            };
            if changed != 1 {
                return Err(StoreError::ClipNotFound(id.to_string_repr()));
            }
        }
        transaction.commit()?;
        if deleted {
            self.scrub_deleted_pages()?;
        }
        Ok(mutations.len())
    }

    /// Delete a single clip by id.
    pub fn delete(&self, id: ClipId) -> Result<()> {
        let deleted = self.conn.execute(
            "DELETE FROM clips WHERE id = ?1",
            params![id.to_string_repr()],
        )?;
        if deleted > 0 {
            self.scrub_deleted_pages()?;
        }
        Ok(())
    }

    /// Delete every non-pinned clip. Pinned clips are preserved.
    pub fn clear(&self) -> Result<()> {
        let deleted = self
            .conn
            .execute("DELETE FROM clips WHERE pinned = 0", [])?;
        if deleted > 0 {
            self.scrub_deleted_pages()?;
        }
        Ok(())
    }

    /// Delete every clip, including pinned ones.
    pub fn clear_all(&self) -> Result<()> {
        let deleted = self.conn.execute("DELETE FROM clips", [])?;
        if deleted > 0 {
            self.scrub_deleted_pages()?;
        }
        Ok(())
    }

    /// Total number of stored clips.
    pub fn count(&self) -> Result<usize> {
        self.purge_expired()?;
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM clips", [], |row| row.get(0))?;
        Ok(n as usize)
    }

    /// Delete clips whose hard privacy TTL elapsed, including pinned rows.
    pub fn purge_expired(&self) -> Result<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM clips WHERE expires_at IS NOT NULL AND expires_at <= ?1",
            params![now_millis()],
        )?;
        if deleted > 0 {
            self.scrub_deleted_pages()?;
        }
        Ok(deleted)
    }

    /// Persist one capture-path outcome without retaining clipboard content.
    pub fn record_capture_outcome(&self, outcome: CaptureOutcome, count: u64) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO capture_metrics(metric, count) VALUES (?1, ?2)
            ON CONFLICT(metric) DO UPDATE SET count =
                CASE
                    WHEN capture_metrics.count >= 9223372036854775807 - excluded.count
                    THEN 9223372036854775807
                    ELSE capture_metrics.count + excluded.count
                END
            "#,
            params![outcome.metric_key(), count.min(i64::MAX as u64) as i64],
        )?;
        Ok(())
    }

    /// Return the durable, content-free capture accounting ledger.
    pub fn capture_metrics(&self) -> Result<BTreeMap<String, u64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT metric, count FROM capture_metrics ORDER BY metric")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?;
        let mut metrics = BTreeMap::new();
        for row in rows {
            let (metric, count) = row?;
            metrics.insert(metric, count);
        }
        Ok(metrics)
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
        if deleted > 0 {
            self.scrub_deleted_pages()?;
        }
        Ok(deleted)
    }

    fn scrub_deleted_pages(&self) -> Result<()> {
        let busy: i64 = self
            .conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
        if busy != 0 {
            return Err(StoreError::Maintenance(
                "WAL truncate checkpoint was busy after deletion".into(),
            ));
        }
        Ok(())
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

fn prepare_private_database_path(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(StoreError::Io)?;
        harden_directory_permissions(parent)?;
    }
    if path.exists() {
        harden_file_permissions(path)?;
    }
    Ok(())
}

fn harden_database_files(path: &Path) -> Result<()> {
    harden_file_permissions(path)?;
    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{suffix}", path.to_string_lossy()));
        if sidecar.exists() {
            harden_file_permissions(&sidecar)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn harden_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(StoreError::Io)
}

#[cfg(not(unix))]
fn harden_directory_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn harden_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(StoreError::Io)
}

#[cfg(not(unix))]
fn harden_file_permissions(_path: &Path) -> Result<()> {
    Ok(())
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
    metadata_json: String,
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
        metadata_json: row.get(8)?,
        pinned: row.get::<_, i64>(9)? != 0,
        favorite: row.get::<_, i64>(10)? != 0,
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
    let created_at =
        chrono::DateTime::from_timestamp_millis(raw.created_at).unwrap_or_else(chrono::Utc::now);
    let stored_meta: StoredMetadata = serde_json::from_str(&raw.metadata_json)?;
    let mut meta = ClipMeta::now(
        kind_from_int(raw.kind),
        raw.byte_size as u64,
        raw.source_app,
    );
    meta.created_at = created_at;
    stored_meta.apply_to(&mut meta);
    Ok(Clip {
        id,
        flavors,
        content_hash,
        meta,
        pinned: raw.pinned,
        favorite: raw.favorite,
    })
}

/// Metadata added after the v1 schema. Core query columns stay normalized,
/// while optional capture context can evolve without a migration per field.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct StoredMetadata {
    provenance: CaptureProvenance,
    generation: Option<CaptureGeneration>,
    lineage: CaptureLineage,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    sensitive: bool,
    sync_eligible: Option<bool>,
}

impl From<&ClipMeta> for StoredMetadata {
    fn from(meta: &ClipMeta) -> Self {
        Self {
            provenance: meta.provenance.clone(),
            generation: meta.generation,
            lineage: meta.lineage.clone(),
            expires_at: meta.expires_at,
            sensitive: meta.sensitive,
            sync_eligible: Some(meta.sync_eligible),
        }
    }
}

impl StoredMetadata {
    fn apply_to(self, meta: &mut ClipMeta) {
        meta.provenance = self.provenance;
        meta.generation = self.generation;
        meta.lineage = self.lineage;
        meta.expires_at = self.expires_at;
        meta.sensitive = self.sensitive;
        meta.sync_eligible = self.sync_eligible.unwrap_or(true);
    }
}

fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn elapsed_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn query_count(connection: &Connection, sql: &str) -> Result<usize> {
    let count: i64 = connection.query_row(sql, [], |row| row.get(0))?;
    Ok(count.max(0) as usize)
}

fn searchable_projection(clip: &Clip, max_bytes: usize) -> String {
    let mut parts = Vec::new();
    if let Some(text) = clip.primary_text() {
        parts.push(text);
    }
    if let Some(source_app) = clip.meta.source_app.as_deref() {
        parts.push(source_app);
    }
    parts.push(clip.meta.kind.label());
    let mut projection = parts.join("\n");
    if projection.len() > max_bytes {
        let mut boundary = max_bytes.min(projection.len());
        while !projection.is_char_boundary(boundary) {
            boundary -= 1;
        }
        projection.truncate(boundary);
    }
    projection
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
    use chrono::Duration;
    use vbuff_core::capture::{CaptureOutcome, DropReason};
    use vbuff_core::content_hash_from_flavors;
    use vbuff_types::{
        Body, CaptureGeneration, CaptureLineage, CaptureProvenance, ClipMeta, ContentKind, Flavor,
    };

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

    #[test]
    fn extended_metadata_and_expiry_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        let mut clip = make_clip("123456");
        clip.meta.provenance = CaptureProvenance {
            app_id: Some("browser.app".into()),
            window_title: Some("Verification".into()),
            source_url: Some("https://login.example.test".into()),
            ..Default::default()
        };
        clip.meta.generation = Some(CaptureGeneration {
            epoch: 4,
            sequence: 9,
        });
        clip.meta.lineage = CaptureLineage {
            origin_device: Some("laptop".into()),
            write_nonce: Some("nonce".into()),
        };
        clip.meta.sensitive = true;
        clip.meta.sync_eligible = false;
        clip.meta.expires_at = Some(chrono::Utc::now() + Duration::minutes(1));

        store.insert(&clip).unwrap();
        let loaded = store.list(1).unwrap().pop().unwrap();
        assert_eq!(loaded.meta.provenance, clip.meta.provenance);
        assert_eq!(loaded.meta.generation, clip.meta.generation);
        assert_eq!(loaded.meta.lineage, clip.meta.lineage);
        assert!(loaded.meta.sensitive);
        assert!(!loaded.meta.sync_eligible);
        assert_eq!(loaded.meta.expires_at, clip.meta.expires_at);
    }

    #[test]
    fn expired_clip_is_never_returned_or_counted() {
        let store = Store::open_in_memory().unwrap();
        let mut clip = make_clip("654321");
        clip.meta.expires_at = Some(chrono::Utc::now() - Duration::seconds(1));
        store.insert(&clip).unwrap();

        assert!(store.list(10).unwrap().is_empty());
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn capture_metrics_accumulate_without_content() {
        let store = Store::open_in_memory().unwrap();
        store
            .record_capture_outcome(CaptureOutcome::Captured, 2)
            .unwrap();
        store
            .record_capture_outcome(CaptureOutcome::Dropped(DropReason::GenerationGap), 3)
            .unwrap();

        assert_eq!(
            store.capture_metrics().unwrap(),
            BTreeMap::from([("captured".into(), 2), ("dropped:generation_gap".into(), 3),])
        );
    }

    #[test]
    fn migrates_v1_schema_without_losing_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE clips (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                id TEXT NOT NULL UNIQUE,
                content_hash BLOB NOT NULL,
                flavors TEXT NOT NULL,
                kind INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                byte_size INTEGER NOT NULL,
                source_app TEXT,
                preview TEXT NOT NULL DEFAULT '',
                pinned INTEGER NOT NULL DEFAULT 0,
                favorite INTEGER NOT NULL DEFAULT 0
            );
            PRAGMA user_version = 1;
            "#,
        )
        .unwrap();

        let store = Store::from_connection(conn).unwrap();
        store.insert(&make_clip("after migration")).unwrap();
        assert_eq!(store.count().unwrap(), 1);
        let version: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        assert!(store.capture_metrics().unwrap().is_empty());
    }

    #[test]
    fn failed_migration_rolls_back_every_schema_step() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE clips (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                id TEXT NOT NULL UNIQUE,
                content_hash BLOB NOT NULL,
                flavors TEXT NOT NULL,
                kind INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                byte_size INTEGER NOT NULL,
                source_app TEXT,
                preview TEXT NOT NULL DEFAULT '',
                metadata_json TEXT NOT NULL DEFAULT '{}',
                expires_at INTEGER,
                pinned INTEGER NOT NULL DEFAULT 0,
                favorite INTEGER NOT NULL DEFAULT 0
            );
            CREATE VIEW clip_facets AS SELECT 1 AS blocked;
            PRAGMA user_version = 2;
            "#,
        )
        .unwrap();
        let mut store = Store {
            conn,
            cas: None,
            dedup_filter: RefCell::new(BloomFilter::with_capacity(1, 10)),
            search_planner: RefCell::new(search::SearchPlanner::default()),
        };

        assert!(store.migrate().is_err());
        let version: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let simhash_exists: bool = store
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('clips') WHERE name = 'simhash')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 2);
        assert!(!simhash_exists);
    }

    #[test]
    fn v4_migration_rebuilds_cross_kind_blob_refcounts_from_flavors() {
        let mut store = Store::open_in_memory().unwrap();
        let blob_ref = "ab".repeat(32);
        for (index, kind) in [ContentKind::Text, ContentKind::Image]
            .into_iter()
            .enumerate()
        {
            let mut clip = make_clip(&format!("blob {index}"));
            clip.content_hash = [index as u8 + 1; 32];
            clip.flavors = vec![Flavor {
                mime: "application/octet-stream".into(),
                body: Body::Spilled {
                    blob_ref: blob_ref.clone(),
                    byte_size: 42,
                },
                origin: Default::default(),
                realization: Default::default(),
                integrity_hash: None,
            }];
            clip.meta.kind = kind;
            clip.meta.byte_size = 42;
            store.insert(&clip).unwrap();
        }
        store
            .conn
            .execute_batch(
                r#"
                DROP TRIGGER clips_blob_ai;
                DROP TRIGGER clips_blob_ad;
                DROP TRIGGER clips_blob_au;
                DROP TABLE blob_refs;
                CREATE TABLE blob_refs (
                    hash TEXT PRIMARY KEY,
                    kind INTEGER NOT NULL,
                    byte_size INTEGER NOT NULL,
                    refcount INTEGER NOT NULL
                );
                INSERT INTO blob_refs VALUES ('stale', 0, 1, 99);
                PRAGMA user_version = 3;
                "#,
            )
            .unwrap();

        store.migrate().unwrap();

        let mut statement = store
            .conn
            .prepare("SELECT hash, kind, refcount FROM blob_refs ORDER BY kind")
            .unwrap();
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(rows, vec![(blob_ref.clone(), 0, 1), (blob_ref, 3, 1)]);
    }

    #[test]
    fn adaptive_fts_finds_code_identifier_fragments() {
        let store = Store::open_in_memory().unwrap();
        for index in 0..260 {
            store
                .insert(&make_clip(&format!("ordinary prose {index}")))
                .unwrap();
        }
        let mut code = make_clip("fn getUserById(id: u64) -> User");
        code.meta.kind = ContentKind::Code;
        store.insert(&code).unwrap();

        let hits = store.search("UserBy", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].primary_text(), code.primary_text());
    }

    #[test]
    fn structured_facets_filter_without_regex_scans() {
        let store = Store::open_in_memory().unwrap();
        let mut url = make_clip("https://docs.rs/rusqlite/latest/rusqlite/");
        url.meta.kind = ContentKind::Url;
        store.insert(&url).unwrap();
        store.insert(&make_clip("https://example.com")).unwrap();

        let hits = store.search("host:docs.rs", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].primary_text(), url.primary_text());
    }

    #[test]
    fn indexed_simhash_returns_exact_and_near_candidates() {
        let store = Store::open_in_memory().unwrap();
        let clip = make_clip("the quick brown fox jumps over the lazy dog");
        store.insert(&clip).unwrap();

        let hits = store
            .find_near_text("the quick brown fox jumps over the lazy dog", 0, 10)
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, clip.id);
    }

    #[test]
    fn fingerprint_backfill_is_bounded_for_fast_startup() {
        let store = Store::open_in_memory().unwrap();
        for text in ["first old row", "second old row", "third old row"] {
            store.insert(&make_clip(text)).unwrap();
        }
        store
            .conn
            .execute(
                r#"
                UPDATE clips SET simhash = NULL, simhash_b0 = NULL, simhash_b1 = NULL,
                                 simhash_b2 = NULL, simhash_b3 = NULL
                "#,
                [],
            )
            .unwrap();

        assert_eq!(store.backfill_fingerprints(1).unwrap(), 1);
        let pending: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM clips WHERE simhash IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pending, 2);
    }

    #[test]
    fn near_fingerprint_falls_back_when_every_band_differs() {
        let store = Store::open_in_memory().unwrap();
        let clip = make_clip("four changed bands");
        store.insert(&clip).unwrap();
        let candidate = 0x0001_0001_0001_0001_u64;
        let bands = fingerprint_bands(candidate);
        store
            .conn
            .execute(
                r#"
                UPDATE clips SET simhash = ?1, simhash_b0 = ?2, simhash_b1 = ?3,
                                 simhash_b2 = ?4, simhash_b3 = ?5
                WHERE id = ?6
                "#,
                params![
                    candidate as i64,
                    i64::from(bands[0]),
                    i64::from(bands[1]),
                    i64::from(bands[2]),
                    i64::from(bands[3]),
                    clip.id.to_string_repr(),
                ],
            )
            .unwrap();

        let hits = store.find_near_fingerprint("simhash", 0, 4, 1).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, clip.id);
        assert!(
            store
                .find_near_fingerprint("simhash", 0, 4, 0)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn local_embeddings_are_lazy_and_rerank_candidates() {
        let store = Store::open_in_memory().unwrap();
        store
            .insert(&make_clip("rust sqlite clipboard search"))
            .unwrap();
        store
            .insert(&make_clip("banana tropical fruit recipe"))
            .unwrap();
        assert_eq!(store.backfill_embeddings(10).unwrap(), 2);
        assert_eq!(store.backfill_embeddings(10).unwrap(), 0);

        let hits = store.local_similarity_search("rust clipboard", 1).unwrap();
        assert_eq!(hits[0].primary_text(), Some("rust sqlite clipboard search"));
        assert_eq!(
            store
                .local_similarity_search("rust clipboard", usize::MAX)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn fts_maintenance_runs_only_after_dirty_threshold() {
        let store = Store::open_in_memory().unwrap();
        store.insert(&make_clip("dirty index")).unwrap();
        let health = store.fts_health().unwrap();
        assert!(health.is_healthy());
        assert_eq!(health.dirty_writes, 1);
        assert!(store.maintain_search_index(1).unwrap());
        assert!(!store.maintain_search_index(1).unwrap());
        assert!(!store.maintain_search_index(0).unwrap());
    }

    #[test]
    fn doctor_reports_schema_integrity_and_unencrypted_backend_truthfully() {
        let store = Store::open_in_memory().unwrap();
        store.insert(&make_clip("doctor row")).unwrap();

        let report = store.doctor().unwrap();

        assert!(report.is_healthy());
        assert_eq!(report.clip_rows, 1);
        assert_eq!(report.cipher_version, None);
    }

    #[test]
    fn sensitive_clawback_removes_search_projection_and_sets_ttl() {
        let store = Store::open_in_memory().unwrap();
        store
            .insert(&make_clip("ghp_abcdefghijklmnopqrstuvwxyz123456"))
            .unwrap();

        let report = store
            .clawback_sensitive(10, std::time::Duration::from_secs(300))
            .unwrap();

        assert_eq!(report.scanned, 1);
        assert_eq!(report.reclassified, 1);
        assert!(
            store
                .search("abcdefghijklmnopqrstuvwxyz123456", 10)
                .unwrap()
                .is_empty()
        );
        let clip = store.list(1).unwrap().pop().unwrap();
        assert!(clip.meta.sensitive);
        assert!(!clip.meta.sync_eligible);
        assert!(clip.meta.expires_at.is_some());
    }

    #[test]
    fn sensitive_clawback_cursor_reaches_rows_beyond_the_first_batch() {
        let store = Store::open_in_memory().unwrap();
        store.insert(&make_clip("ordinary row one")).unwrap();
        store.insert(&make_clip("ordinary row two")).unwrap();
        store
            .insert(&make_clip("ghp_abcdefghijklmnopqrstuvwxyz123456"))
            .unwrap();

        assert_eq!(
            store
                .clawback_sensitive(2, std::time::Duration::from_secs(300))
                .unwrap()
                .reclassified,
            0
        );
        assert_eq!(
            store
                .clawback_sensitive(2, std::time::Duration::from_secs(300))
                .unwrap()
                .reclassified,
            1
        );
    }

    #[test]
    fn batch_mutation_rolls_back_every_prior_change_on_missing_id() {
        let store = Store::open_in_memory().unwrap();
        let first = store.insert(&make_clip("first batch row")).unwrap();
        let second = store.insert(&make_clip("second batch row")).unwrap();
        let missing = ClipId::new();

        let error = store
            .apply_batch(&[
                StoreMutation::SetPinned {
                    id: first,
                    pinned: true,
                },
                StoreMutation::SetFavorite {
                    id: missing,
                    favorite: true,
                },
            ])
            .unwrap_err();
        assert!(matches!(error, StoreError::ClipNotFound(_)));
        assert!(
            !store
                .list(10)
                .unwrap()
                .iter()
                .find(|clip| clip.id == first)
                .unwrap()
                .pinned
        );

        assert_eq!(
            store
                .apply_batch(&[
                    StoreMutation::SetPinned {
                        id: first,
                        pinned: true,
                    },
                    StoreMutation::Delete { id: second },
                ])
                .unwrap(),
            2
        );
        assert_eq!(store.count().unwrap(), 1);
        assert!(store.list(1).unwrap()[0].pinned);
    }

    #[test]
    fn profiled_open_does_not_claim_missing_encryption_or_kdf() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("profiled.db");

        let (store, profile) = Store::open_profiled(&path).unwrap();

        assert_eq!(store.count().unwrap(), 0);
        assert!(!profile.encryption_enabled);
        assert_eq!(profile.kdf_ms, None);
        drop(store);

        let (read_only, read_only_profile) = Store::open_read_only_profiled(&path).unwrap();
        assert_eq!(read_only.doctor().unwrap().clip_rows, 0);
        assert!(!read_only_profile.encryption_enabled);
        assert!(read_only.insert(&make_clip("must fail")).is_err());
    }

    #[test]
    fn keyset_session_pages_without_duplicates_across_pinned_boundary() {
        let store = Store::open_in_memory().unwrap();
        let mut ids = Vec::new();
        for index in 0..5 {
            let clip = make_clip(&format!("pageable {index}"));
            ids.push(store.insert(&clip).unwrap());
        }
        store.set_pinned(ids[0], true).unwrap();

        let mut session = SearchSession::new("pageable");
        let mut seen = Vec::new();
        loop {
            let page = session.next_page(&store, 2).unwrap();
            seen.extend(page.clips.into_iter().map(|clip| clip.id));
            if page.next_cursor.is_none() {
                break;
            }
        }
        seen.sort_by_key(ClipId::to_string_repr);
        seen.dedup();
        assert_eq!(seen.len(), 5);
    }

    #[test]
    fn rolling_audit_repairs_unique_hash_mismatch() {
        let store = Store::open_in_memory().unwrap();
        let clip = make_clip("repair my hash");
        store.insert(&clip).unwrap();
        store
            .conn
            .execute(
                "UPDATE clips SET content_hash = ?1 WHERE id = ?2",
                params![[99_u8; 32].as_slice(), clip.id.to_string_repr()],
            )
            .unwrap();

        let report = store.audit_content_hashes(10).unwrap();
        assert_eq!(report.repaired, 1);
        assert_eq!(store.list(1).unwrap()[0].content_hash, clip.content_hash);
    }

    #[test]
    fn rolling_audit_quarantines_a_hash_collision_row() {
        let store = Store::open_in_memory().unwrap();
        let canonical = make_clip("canonical bytes");
        let corrupted = make_clip("different bytes");
        store.insert(&canonical).unwrap();
        store.insert(&corrupted).unwrap();
        store
            .conn
            .execute(
                "UPDATE clips SET flavors = ?1 WHERE id = ?2",
                params![
                    serde_clip::flavors_to_json(&canonical.flavors).unwrap(),
                    corrupted.id.to_string_repr(),
                ],
            )
            .unwrap();

        let report = store.audit_content_hashes(10).unwrap();
        assert_eq!(report.quarantined, 1);
        assert_eq!(store.count().unwrap(), 1);
        let (quarantined, record): (i64, String) = store
            .conn
            .query_row(
                "SELECT COUNT(*), MIN(row_json) FROM quarantined_clips",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(quarantined, 1);
        let record: serde_json::Value = serde_json::from_str(&record).unwrap();
        assert_eq!(record["id"], corrupted.id.to_string_repr());
        assert!(record.get("flavors").is_none());
        assert!(record.get("content_hash").is_none());
    }

    #[test]
    fn capture_metrics_saturate_at_sqlite_integer_max() {
        let store = Store::open_in_memory().unwrap();
        store
            .conn
            .execute(
                "INSERT INTO capture_metrics(metric, count) VALUES ('captured', ?1)",
                [i64::MAX - 1],
            )
            .unwrap();

        store
            .record_capture_outcome(CaptureOutcome::Captured, 10)
            .unwrap();

        assert_eq!(
            store.capture_metrics().unwrap()["captured"],
            i64::MAX as u64
        );
    }
}
