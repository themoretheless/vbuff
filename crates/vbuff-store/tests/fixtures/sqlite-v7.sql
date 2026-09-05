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
                ALTER TABLE clips ADD COLUMN normalized_hash BLOB;
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
                content_hash BLOB NOT NULL REFERENCES clips(content_hash)
                    ON UPDATE CASCADE ON DELETE CASCADE,
                backend_id TEXT NOT NULL,
                dimensions INTEGER NOT NULL,
                scale REAL NOT NULL,
                vector BLOB NOT NULL,
                PRIMARY KEY (content_hash, backend_id)
            ) WITHOUT ROWID;

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
            INSERT OR IGNORE INTO maintenance_state(key, value) VALUES ('pending_wal_scrub', 0);

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

            CREATE TABLE IF NOT EXISTS dedup_merge_ledger (
                event_seq INTEGER PRIMARY KEY AUTOINCREMENT,
                clip_id TEXT NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
                merged_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_dedup_merge_clip
                ON dedup_merge_ledger(clip_id, merged_at DESC, event_seq DESC);

            CREATE TABLE IF NOT EXISTS grace_bin (
                recovery_id TEXT PRIMARY KEY,
                clip_id TEXT NOT NULL,
                deleted_at INTEGER NOT NULL,
                purge_after INTEGER NOT NULL,
                reason INTEGER NOT NULL CHECK(reason BETWEEN 0 AND 2),
                nonce BLOB NOT NULL CHECK(length(nonce) = 24),
                ciphertext BLOB NOT NULL CHECK(length(ciphertext) >= 16)
            );
            CREATE INDEX IF NOT EXISTS idx_grace_bin_expiry
                ON grace_bin(purge_after, recovery_id);

            CREATE TABLE IF NOT EXISTS retention_rules (
                kind INTEGER NOT NULL,
                sensitive INTEGER NOT NULL CHECK(sensitive IN (0, 1)),
                max_age_ms INTEGER,
                max_items INTEGER,
                grace_ms INTEGER NOT NULL CHECK(grace_ms >= 0),
                PRIMARY KEY(kind, sensitive),
                CHECK(max_age_ms IS NOT NULL OR max_items IS NOT NULL),
                CHECK(max_age_ms IS NULL OR max_age_ms > 0),
                CHECK(max_items IS NULL OR max_items >= 0)
            ) WITHOUT ROWID;

            CREATE TABLE IF NOT EXISTS collection_policies (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                max_age_days INTEGER,
                max_items INTEGER,
                max_bytes INTEGER,
                CHECK(max_age_days IS NOT NULL OR max_items IS NOT NULL OR max_bytes IS NOT NULL),
                CHECK(max_age_days IS NULL OR max_age_days BETWEEN 1 AND 3650),
                CHECK(max_items IS NULL OR max_items BETWEEN 0 AND 1000000),
                CHECK(max_bytes IS NULL OR max_bytes > 0)
            ) WITHOUT ROWID;

            CREATE TABLE IF NOT EXISTS clip_annotations (
                clip_id TEXT PRIMARY KEY REFERENCES clips(id) ON DELETE CASCADE,
                archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN (0, 1)),
                collection_id TEXT REFERENCES collection_policies(id) ON DELETE SET NULL,
                preferred_mime TEXT,
                legal_hold INTEGER NOT NULL DEFAULT 0 CHECK(legal_hold IN (0, 1))
            ) WITHOUT ROWID;
            CREATE INDEX IF NOT EXISTS idx_clip_annotations_archive
                ON clip_annotations(archived, clip_id);
            CREATE INDEX IF NOT EXISTS idx_clip_annotations_collection
                ON clip_annotations(collection_id, clip_id)
                WHERE collection_id IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_clip_annotations_hold
                ON clip_annotations(legal_hold, clip_id)
                WHERE legal_hold = 1;

            CREATE TABLE IF NOT EXISTS clip_residency (
                clip_id TEXT PRIMARY KEY REFERENCES clips(id) ON DELETE CASCADE,
                ever_on_disk INTEGER NOT NULL DEFAULT 1 CHECK(ever_on_disk IN (0, 1)),
                ever_synced INTEGER NOT NULL DEFAULT 0 CHECK(ever_synced IN (0, 1)),
                ever_exported INTEGER NOT NULL DEFAULT 0 CHECK(ever_exported IN (0, 1))
            ) WITHOUT ROWID;

            CREATE TABLE IF NOT EXISTS blob_quarantine (
                hash TEXT NOT NULL,
                kind INTEGER NOT NULL,
                quarantined_at INTEGER NOT NULL,
                reason TEXT NOT NULL,
                PRIMARY KEY(hash, kind)
            ) WITHOUT ROWID;

            CREATE TABLE IF NOT EXISTS backup_state (
                singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                verified_at INTEGER NOT NULL,
                checksum TEXT NOT NULL CHECK(length(checksum) = 64)
            );

            CREATE TABLE IF NOT EXISTS import_quarantine (
                import_id TEXT PRIMARY KEY,
                source_fingerprint TEXT NOT NULL,
                clip_id TEXT NOT NULL,
                staged_at INTEGER NOT NULL,
                byte_size INTEGER NOT NULL CHECK(byte_size >= 0),
                sensitive INTEGER NOT NULL CHECK(sensitive IN (0, 1)),
                payload_json TEXT NOT NULL
            ) WITHOUT ROWID;
            CREATE INDEX IF NOT EXISTS idx_import_quarantine_staged
                ON import_quarantine(staged_at, import_id);

            CREATE TRIGGER IF NOT EXISTS clips_lifecycle_ai AFTER INSERT ON clips BEGIN
                INSERT OR IGNORE INTO clip_annotations(clip_id) VALUES (new.id);
                INSERT OR IGNORE INTO clip_residency(clip_id, ever_on_disk) VALUES (new.id, 1);
            END;

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
            CREATE INDEX IF NOT EXISTS idx_clips_normalized_hash
                ON clips(normalized_hash, updated_at DESC)
                WHERE normalized_hash IS NOT NULL;

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

PRAGMA user_version = 7;
