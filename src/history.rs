//! Application-layer access to history and the GUI snapshot.
//!
//! The SQLite store remains the source of truth. This facade keeps mutex and
//! snapshot-refresh plumbing out of capture, tray, and command handling.

use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use vbuff_core::capture::CaptureOutcome;
use vbuff_gui::SharedState;
use vbuff_store::Store;
use vbuff_types::{Clip, ClipId};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MaintenanceSummary {
    pub fingerprints: usize,
    pub embeddings: usize,
    pub audited: usize,
    pub repaired: usize,
    pub quarantined: usize,
    pub expired: usize,
    pub blobs_collected: usize,
    pub fts_optimized: bool,
}

/// Cloneable history handle shared by the capture and UI threads.
#[derive(Clone)]
pub(crate) struct History {
    store: Arc<Mutex<Store>>,
    shared: SharedState,
    snapshot_limit: usize,
}

impl History {
    pub(crate) fn new(store: Store, shared: SharedState, snapshot_limit: usize) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            shared,
            snapshot_limit,
        }
    }

    /// Insert a captured clip, enforce retention, and publish a fresh snapshot.
    pub(crate) fn insert(&self, clip: &Clip, max_history: usize) -> anyhow::Result<()> {
        self.mutate_and_refresh(|store| {
            store.insert(clip)?;
            store.enforce_cap(max_history)?;
            Ok(())
        })
    }

    pub(crate) fn record_capture_outcome(
        &self,
        outcome: CaptureOutcome,
        count: u64,
    ) -> anyhow::Result<()> {
        self.store
            .lock()
            .map_err(|_| anyhow!("history store mutex poisoned"))?
            .record_capture_outcome(outcome, count)?;
        Ok(())
    }

    pub(crate) fn set_pinned(&self, id: ClipId, pinned: bool) -> anyhow::Result<()> {
        self.mutate_and_refresh(|store| store.set_pinned(id, pinned))
    }

    pub(crate) fn delete(&self, id: ClipId) -> anyhow::Result<()> {
        self.mutate_and_refresh(|store| store.delete(id))
    }

    /// Clear non-pinned history. The command name is shared across all surfaces.
    pub(crate) fn clear_history(&self) -> anyhow::Result<()> {
        self.mutate_and_refresh(Store::clear)
    }

    pub(crate) fn find(&self, id: ClipId) -> anyhow::Result<Option<Clip>> {
        let state = self
            .shared
            .lock()
            .map_err(|_| anyhow!("GUI state mutex poisoned"))?;
        Ok(state.clips.iter().find(|clip| clip.id == id).cloned())
    }

    #[cfg(feature = "tray")]
    pub(crate) fn latest(&self) -> anyhow::Result<Option<Clip>> {
        let state = self
            .shared
            .lock()
            .map_err(|_| anyhow!("GUI state mutex poisoned"))?;
        Ok(state.clips.first().cloned())
    }

    pub(crate) fn maintain_idle(&self) -> anyhow::Result<Option<MaintenanceSummary>> {
        let (summary, refreshed_clips) = {
            let store = match self.store.try_lock() {
                Ok(store) => store,
                Err(std::sync::TryLockError::WouldBlock) => return Ok(None),
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err(anyhow!("history store mutex poisoned"));
                }
            };
            let expired = store.purge_expired()?;
            let fingerprints = store.backfill_fingerprints(32)?;
            let embeddings = store.backfill_embeddings(32)?;
            let audit = store.audit_content_hashes(32)?;
            let fts_optimized = store.maintain_search_index(256)?;
            let blobs_collected = store.gc_blobs()?;
            let changed_visible_rows = expired > 0 || audit.repaired > 0 || audit.quarantined > 0;
            let refreshed_clips = changed_visible_rows
                .then(|| store.load_recent(self.snapshot_limit))
                .transpose()?;
            (
                MaintenanceSummary {
                    fingerprints,
                    embeddings,
                    audited: audit.checked,
                    repaired: audit.repaired,
                    quarantined: audit.quarantined,
                    expired,
                    blobs_collected,
                    fts_optimized,
                },
                refreshed_clips,
            )
        };

        if let Some(clips) = refreshed_clips {
            self.shared
                .lock()
                .map_err(|_| anyhow!("GUI state mutex poisoned"))?
                .set_clips(clips);
        }
        Ok(Some(summary))
    }

    fn mutate_and_refresh(
        &self,
        mutation: impl FnOnce(&Store) -> vbuff_store::Result<()>,
    ) -> anyhow::Result<()> {
        let clips = {
            let store = self
                .store
                .lock()
                .map_err(|_| anyhow!("history store mutex poisoned"))?;
            mutation(&store)?;
            store.load_recent(self.snapshot_limit)?
        };

        self.shared
            .lock()
            .map_err(|_| anyhow!("GUI state mutex poisoned"))?
            .set_clips(clips);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use vbuff_gui::AppState;
    use vbuff_types::{ClipMeta, ContentKind, Flavor};

    use super::*;

    #[test]
    fn maintenance_removes_expired_clips_from_the_gui_snapshot() {
        let flavors = vec![Flavor::inline("text/plain", b"123456".to_vec())];
        let mut meta = ClipMeta::now(ContentKind::Text, 6, None);
        meta.expires_at = Some(Utc::now() - Duration::seconds(1));
        meta.sensitive = true;
        meta.sync_eligible = false;
        let clip = Clip {
            id: ClipId::new(),
            content_hash: vbuff_core::content_hash_from_flavors(&flavors),
            flavors,
            meta,
            pinned: false,
            favorite: false,
        };
        let store = Store::open_in_memory().unwrap();
        store.insert(&clip).unwrap();
        let shared = Arc::new(Mutex::new(AppState::with_clips(vec![clip])));
        let history = History::new(store, Arc::clone(&shared), 100);

        let summary = history.maintain_idle().unwrap().unwrap();

        assert_eq!(summary.expired, 1);
        assert!(shared.lock().unwrap().clips.is_empty());
    }
}
