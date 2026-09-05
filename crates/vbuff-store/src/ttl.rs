//! Explicit expiry edits keep the SQL projection and serialized metadata atomic.
use crate::{Result, Store, StoreError, StoredMetadata, params};
use vbuff_types::ClipId;
impl Store {
    /// Set a normal clip's lifetime from now, or remove its explicit deadline.
    /// Privacy TTLs on sensitive clips are managed by capture policy instead.
    pub fn set_ttl(&self, id: ClipId, seconds: Option<u64>) -> Result<()> {
        if seconds.is_some_and(|s| s == 0 || s > 365 * 24 * 3600) {
            return Err(StoreError::Maintenance(
                "TTL must be between one second and one year".into(),
            ));
        }
        let now = chrono::Utc::now();
        let tx = self.conn.unchecked_transaction()?;
        let json: String = tx.query_row("SELECT metadata_json FROM clips WHERE id = $1 AND (expires_at IS NULL OR expires_at > $2)", params![id.to_string_repr(), now.timestamp_millis()], |r| r.get(0))?;
        let mut metadata: StoredMetadata = serde_json::from_str(&json)?;
        if metadata.sensitive {
            return Err(StoreError::Maintenance(
                "Sensitive expiry is controlled by privacy policy".into(),
            ));
        }
        metadata.expires_at = seconds.map(|s| now + chrono::Duration::seconds(s as i64));
        tx.execute(
            "UPDATE clips SET expires_at = $1, metadata_json = $2 WHERE id = $3",
            params![
                metadata.expires_at.map(|t| t.timestamp_millis()),
                serde_json::to_string(&metadata)?,
                id.to_string_repr()
            ],
        )?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_types::{Clip, ClipMeta, ContentKind, Flavor};
    fn clip() -> Clip {
        let flavors = vec![Flavor::inline(
            "text/plain",
            b"temporary pinned note".to_vec(),
        )];
        Clip {
            id: ClipId::new(),
            content_hash: vbuff_core::content_hash_from_flavors(&flavors),
            meta: ClipMeta::now(ContentKind::Text, 21, None),
            flavors,
            pinned: false,
            favorite: false,
        }
    }
    #[test]
    fn ttl_survives_recopy_reopen_and_can_be_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let clip = clip();
        let deadline;
        {
            let store = Store::open(&path).unwrap();
            store.insert(&clip).unwrap();
            store.set_pinned(clip.id, true).unwrap();
            store.set_ttl(clip.id, Some(3600)).unwrap();
            deadline = store.get_clip(clip.id).unwrap().unwrap().meta.expires_at;
            assert!(deadline.is_some());
            store.insert(&clip).unwrap();
            assert_eq!(
                store.get_clip(clip.id).unwrap().unwrap().meta.expires_at,
                deadline
            );
        }
        let store = Store::open(&path).unwrap();
        let restored = store.get_clip(clip.id).unwrap().unwrap();
        assert_eq!(restored.meta.expires_at, deadline);
        assert!(restored.pinned);
        assert!(store.set_ttl(clip.id, Some(0)).is_err());
        assert_eq!(
            store.get_clip(clip.id).unwrap().unwrap().meta.expires_at,
            deadline
        );
        store.set_ttl(clip.id, None).unwrap();
        assert!(
            store
                .get_clip(clip.id)
                .unwrap()
                .unwrap()
                .meta
                .expires_at
                .is_none()
        );
    }
    #[test]
    fn expiry_wins_over_pin_and_session_protection() {
        let store = Store::open_in_memory().unwrap();
        let clip = clip();
        store.insert(&clip).unwrap();
        store.set_pinned(clip.id, true).unwrap();
        store.set_session_protected(clip.id, true).unwrap();
        store.set_ttl(clip.id, Some(1)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert!(store.get_clip(clip.id).unwrap().is_none());
        store.purge_expired().unwrap();
        let count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM clips", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
    #[test]
    fn manual_ttl_cannot_remove_sensitive_deadline() {
        let store = Store::open_in_memory().unwrap();
        let mut clip = clip();
        clip.meta.sensitive = true;
        store.insert(&clip).unwrap();
        let before = store.get_clip(clip.id).unwrap().unwrap().meta.expires_at;
        assert!(store.set_ttl(clip.id, None).is_err());
        assert_eq!(
            store.get_clip(clip.id).unwrap().unwrap().meta.expires_at,
            before
        );
    }
}
