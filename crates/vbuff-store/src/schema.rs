//! Schema definition and forward-only migrations.
//!
//! The MVP schema is deliberately compact: a single `clips` table whose
//! `flavors` column holds the flavor set as a JSON blob. This keeps reads to a
//! single row fetch and avoids a join on the hot path; the full normalized
//! `item`/`flavor` split from the architecture can be migrated to later.

use rusqlite::Connection;

use crate::Result;

/// The current schema version, stored in `PRAGMA user_version`.
pub(crate) const SCHEMA_VERSION: i64 = 1;

/// Apply forward-only migrations based on `user_version`.
pub(crate) fn migrate(conn: &Connection) -> Result<()> {
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
                pinned       INTEGER NOT NULL DEFAULT 0,
                favorite     INTEGER NOT NULL DEFAULT 0
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_clips_hash ON clips(content_hash);
            CREATE INDEX IF NOT EXISTS idx_clips_updated ON clips(updated_at DESC, seq DESC);
            CREATE INDEX IF NOT EXISTS idx_clips_pinned ON clips(updated_at DESC) WHERE pinned = 1;
            "#,
        )?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(())
}
