CREATE TABLE IF NOT EXISTS clips (
                    seq          BIGINT PRIMARY KEY, -- definitive recency tiebreaker
                    id           TEXT NOT NULL UNIQUE,    -- ULID string
                    content_hash BLOB NOT NULL,           -- 32-byte BLAKE3 digest
                    flavors      TEXT NOT NULL,           -- JSON array of flavors
                    kind         BIGINT NOT NULL,        -- ContentKind discriminant
                    created_at   BIGINT NOT NULL,        -- epoch millis (UTC)
                    updated_at   BIGINT NOT NULL,        -- bumped on re-copy (move to top)
                    byte_size    BIGINT NOT NULL,
                    source_app   TEXT,
                    preview      TEXT NOT NULL DEFAULT '',-- cached search/preview text
                    item_text    TEXT NOT NULL DEFAULT '',-- bounded full-text projection
                    metadata_json TEXT NOT NULL DEFAULT '{}',
                    expires_at   BIGINT,
                    simhash      BIGINT,
                    simhash_b0   BIGINT,
                    simhash_b1   BIGINT,
                    simhash_b2   BIGINT,
                    simhash_b3   BIGINT,
                    dhash        BIGINT,
                    dhash_b0     BIGINT,
                    dhash_b1     BIGINT,
                    dhash_b2     BIGINT,
                    dhash_b3     BIGINT,
                    pinned       BIGINT NOT NULL DEFAULT 0,
                    favorite     BIGINT NOT NULL DEFAULT 0
                );
                CREATE UNIQUE INDEX IF NOT EXISTS idx_clips_hash ON clips(content_hash);
                CREATE INDEX IF NOT EXISTS idx_clips_updated ON clips(updated_at DESC, seq DESC);
                CREATE INDEX IF NOT EXISTS idx_clips_pinned ON clips(updated_at DESC);
                CREATE TABLE IF NOT EXISTS capture_metrics (
                metric TEXT PRIMARY KEY,
                count  BIGINT NOT NULL CHECK(count >= 0)
            );

            CREATE TABLE IF NOT EXISTS clip_facets (
                clip_id TEXT NOT NULL ,
                key     TEXT NOT NULL,
                value   TEXT NOT NULL,
                PRIMARY KEY (clip_id, key, value)
            ) ;
            CREATE INDEX IF NOT EXISTS idx_clip_facets_lookup
                ON clip_facets(key, value, clip_id);

            CREATE TABLE IF NOT EXISTS clip_embeddings (
                content_hash BLOB NOT NULL ,
                backend_id TEXT NOT NULL,
                dimensions BIGINT NOT NULL,
                scale DOUBLE NOT NULL,
                vector BLOB NOT NULL,
                PRIMARY KEY (content_hash, backend_id)
            ) ;

            CREATE TABLE IF NOT EXISTS blob_refs (
                hash TEXT NOT NULL,
                kind BIGINT NOT NULL,
                byte_size BIGINT NOT NULL,
                refcount BIGINT NOT NULL CHECK(refcount >= 0),
                PRIMARY KEY (hash, kind)
            ) ;

            CREATE TABLE IF NOT EXISTS maintenance_state (
                key TEXT PRIMARY KEY,
                value BIGINT NOT NULL
            );
            INSERT OR IGNORE INTO maintenance_state(key, value) VALUES ('fts_dirty', 0);
            INSERT OR IGNORE INTO maintenance_state(key, value) VALUES ('secret_scan_cursor', 0);
            INSERT OR IGNORE INTO maintenance_state(key, value) VALUES ('pending_wal_scrub', 0);

            CREATE TABLE IF NOT EXISTS content_audit (
                clip_id TEXT PRIMARY KEY ,
                checked_at BIGINT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS quarantined_clips (
                id TEXT PRIMARY KEY,
                quarantined_at BIGINT NOT NULL,
                reason TEXT NOT NULL,
                row_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS dedup_merge_ledger (
                event_seq BIGINT PRIMARY KEY,
                clip_id TEXT NOT NULL ,
                merged_at BIGINT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_dedup_merge_clip
                ON dedup_merge_ledger(clip_id, merged_at DESC, event_seq DESC);

            CREATE TABLE IF NOT EXISTS grace_bin (
                recovery_id TEXT PRIMARY KEY,
                clip_id TEXT NOT NULL,
                deleted_at BIGINT NOT NULL,
                purge_after BIGINT NOT NULL,
                reason BIGINT NOT NULL CHECK(reason BETWEEN 0 AND 2),
                nonce BLOB NOT NULL CHECK(octet_length(nonce) = 24),
                ciphertext BLOB NOT NULL CHECK(octet_length(ciphertext) >= 16)
            );
            CREATE INDEX IF NOT EXISTS idx_grace_bin_expiry
                ON grace_bin(purge_after, recovery_id);

            CREATE TABLE IF NOT EXISTS retention_rules (
                kind BIGINT NOT NULL,
                sensitive BIGINT NOT NULL CHECK(sensitive IN (0, 1)),
                max_age_ms BIGINT,
                max_items BIGINT,
                grace_ms BIGINT NOT NULL CHECK(grace_ms >= 0),
                PRIMARY KEY(kind, sensitive),
                CHECK(max_age_ms IS NOT NULL OR max_items IS NOT NULL),
                CHECK(max_age_ms IS NULL OR max_age_ms > 0),
                CHECK(max_items IS NULL OR max_items >= 0)
            ) ;

            CREATE TABLE IF NOT EXISTS collection_policies (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                max_age_days BIGINT,
                max_items BIGINT,
                max_bytes BIGINT,
                CHECK(max_age_days IS NOT NULL OR max_items IS NOT NULL OR max_bytes IS NOT NULL),
                CHECK(max_age_days IS NULL OR max_age_days BETWEEN 1 AND 3650),
                CHECK(max_items IS NULL OR max_items BETWEEN 0 AND 1000000),
                CHECK(max_bytes IS NULL OR max_bytes > 0)
            ) ;

            CREATE TABLE IF NOT EXISTS clip_annotations (
                clip_id TEXT PRIMARY KEY ,
                archived BIGINT NOT NULL DEFAULT 0 CHECK(archived IN (0, 1)),
                collection_id TEXT ,
                preferred_mime TEXT,
                legal_hold BIGINT NOT NULL DEFAULT 0 CHECK(legal_hold IN (0, 1))
            ) ;
            CREATE INDEX IF NOT EXISTS idx_clip_annotations_archive
                ON clip_annotations(archived, clip_id);
            CREATE INDEX IF NOT EXISTS idx_clip_annotations_collection
                ON clip_annotations(collection_id, clip_id);
            CREATE INDEX IF NOT EXISTS idx_clip_annotations_hold
                ON clip_annotations(legal_hold, clip_id);

            CREATE TABLE IF NOT EXISTS clip_residency (
                clip_id TEXT PRIMARY KEY ,
                ever_on_disk BIGINT NOT NULL DEFAULT 1 CHECK(ever_on_disk IN (0, 1)),
                ever_synced BIGINT NOT NULL DEFAULT 0 CHECK(ever_synced IN (0, 1)),
                ever_exported BIGINT NOT NULL DEFAULT 0 CHECK(ever_exported IN (0, 1))
            ) ;

            CREATE TABLE IF NOT EXISTS blob_quarantine (
                hash TEXT NOT NULL,
                kind BIGINT NOT NULL,
                quarantined_at BIGINT NOT NULL,
                reason TEXT NOT NULL,
                PRIMARY KEY(hash, kind)
            ) ;

            CREATE TABLE IF NOT EXISTS backup_state (
                singleton BIGINT PRIMARY KEY CHECK(singleton = 1),
                verified_at BIGINT NOT NULL,
                checksum TEXT NOT NULL CHECK(length(checksum) = 64)
            );

            CREATE TABLE IF NOT EXISTS import_quarantine (
                import_id TEXT PRIMARY KEY,
                source_fingerprint TEXT NOT NULL,
                clip_id TEXT NOT NULL,
                staged_at BIGINT NOT NULL,
                byte_size BIGINT NOT NULL CHECK(byte_size >= 0),
                sensitive BIGINT NOT NULL CHECK(sensitive IN (0, 1)),
                payload_json TEXT NOT NULL
            ) ;
            CREATE INDEX IF NOT EXISTS idx_import_quarantine_staged
                ON import_quarantine(staged_at, import_id);


ALTER TABLE clips ADD COLUMN IF NOT EXISTS normalized_hash BLOB;
CREATE TABLE IF NOT EXISTS store_metadata(key TEXT PRIMARY KEY, value BIGINT NOT NULL);
INSERT OR IGNORE INTO store_metadata VALUES ('schema_version', 1);
INSERT OR IGNORE INTO store_metadata VALUES ('clip_sequence', 0), ('merge_sequence', 0);
