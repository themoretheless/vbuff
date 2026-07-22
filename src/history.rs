//! Application-layer access to history and the GUI snapshot.
//!
//! The SQLite store remains the source of truth. This facade keeps mutex and
//! snapshot-refresh plumbing out of capture, tray, and command handling.

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::anyhow;
use chrono::Utc;
use vbuff_core::capture::CaptureOutcome;
use vbuff_gui::SharedState;
use vbuff_store::Store;
use vbuff_types::{Clip, ClipId};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MaintenanceSummary {
    pub fingerprints: usize,
    pub normalized_fingerprints: usize,
    pub embeddings: usize,
    pub audited: usize,
    pub repaired: usize,
    pub quarantined: usize,
    pub reclassified_sensitive: usize,
    pub expired: usize,
    pub blobs_collected: usize,
    pub fts_optimized: bool,
}

/// Cloneable history handle shared by the capture and UI threads.
#[derive(Clone)]
pub(crate) struct History {
    store: Arc<Mutex<Store>>,
    volatile: Arc<Mutex<Vec<Clip>>>,
    volatile_origins: Arc<Mutex<VecDeque<ClipId>>>,
    shared: SharedState,
    snapshot_limit: Arc<AtomicUsize>,
}

impl History {
    pub(crate) fn new(store: Store, shared: SharedState, snapshot_limit: usize) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            volatile: Arc::new(Mutex::new(Vec::new())),
            volatile_origins: Arc::new(Mutex::new(VecDeque::new())),
            shared,
            snapshot_limit: Arc::new(AtomicUsize::new(snapshot_limit.max(1))),
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

    /// Publish a short-lived clip without writing its payload to SQLite.
    pub(crate) fn insert_volatile(&self, clip: Clip) -> anyhow::Result<()> {
        const MAX_VOLATILE_CLIPS: usize = 32;
        const MAX_VOLATILE_ORIGINS: usize = 256;
        {
            let mut origins = self
                .volatile_origins
                .lock()
                .map_err(|_| anyhow!("volatile origin mutex poisoned"))?;
            origins.retain(|id| *id != clip.id);
            origins.push_back(clip.id);
            while origins.len() > MAX_VOLATILE_ORIGINS {
                origins.pop_front();
            }
        }
        {
            let mut volatile = self
                .volatile
                .lock()
                .map_err(|_| anyhow!("volatile history mutex poisoned"))?;
            volatile.retain(|candidate| {
                candidate.content_hash != clip.content_hash && !is_expired(candidate)
            });
            volatile.insert(0, clip);
            volatile.truncate(MAX_VOLATILE_CLIPS);
        }
        self.refresh_snapshot()
    }

    /// Insert one explicit starter pack and refresh the snapshot once.
    pub(crate) fn insert_many(&self, clips: &[Clip], max_history: usize) -> anyhow::Result<()> {
        self.mutate_and_refresh(|store| {
            for clip in clips {
                store.insert(clip)?;
            }
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
        if self.is_memory_only(id)? {
            return Err(anyhow!("memory-only clips cannot be pinned"));
        }
        self.mutate_and_refresh(|store| store.set_pinned(id, pinned))
    }

    pub(crate) fn set_session_protected(&self, id: ClipId, protected: bool) -> anyhow::Result<()> {
        if self.is_memory_only(id)? {
            return Err(anyhow!(
                "memory-only clips cannot receive session protection"
            ));
        }
        self.store
            .lock()
            .map_err(|_| anyhow!("history store mutex poisoned"))?
            .set_session_protected(id, protected)?;
        let mut state = self
            .shared
            .lock()
            .map_err(|_| anyhow!("GUI state mutex poisoned"))?;
        if protected {
            state.session_protected.insert(id);
        } else {
            state.session_protected.remove(&id);
        }
        state.revision = state.revision.wrapping_add(1);
        Ok(())
    }

    pub(crate) fn delete(&self, id: ClipId) -> anyhow::Result<()> {
        let removed_volatile = {
            let mut volatile = self
                .volatile
                .lock()
                .map_err(|_| anyhow!("volatile history mutex poisoned"))?;
            let previous_len = volatile.len();
            volatile.retain(|clip| clip.id != id);
            volatile.len() != previous_len
        };
        if removed_volatile {
            self.refresh_snapshot()?;
            return Ok(());
        }
        self.mutate_and_refresh(|store| store.delete(id))?;
        self.shared
            .lock()
            .map_err(|_| anyhow!("GUI state mutex poisoned"))?
            .session_protected
            .remove(&id);
        Ok(())
    }

    /// Clear non-pinned history. The command name is shared across all surfaces.
    pub(crate) fn clear_history(&self) -> anyhow::Result<()> {
        self.volatile
            .lock()
            .map_err(|_| anyhow!("volatile history mutex poisoned"))?
            .clear();
        self.mutate_and_refresh(Store::clear)
    }

    pub(crate) fn find(&self, id: ClipId) -> anyhow::Result<Option<Clip>> {
        if let Some(clip) = self
            .volatile
            .lock()
            .map_err(|_| anyhow!("volatile history mutex poisoned"))?
            .iter()
            .find(|clip| clip.id == id && !is_expired(clip))
            .cloned()
        {
            return Ok(Some(clip));
        }
        Ok(self
            .store
            .lock()
            .map_err(|_| anyhow!("history store mutex poisoned"))?
            .get_clip(id)?)
    }

    pub(crate) fn is_memory_only(&self, id: ClipId) -> anyhow::Result<bool> {
        Ok(self
            .volatile_origins
            .lock()
            .map_err(|_| anyhow!("volatile origin mutex poisoned"))?
            .contains(&id))
    }

    pub(crate) fn restore(&self, clip: Clip, max_history: usize) -> anyhow::Result<()> {
        if self.is_memory_only(clip.id)? {
            self.insert_volatile(clip)
        } else {
            self.insert(&clip, max_history)
        }
    }

    #[cfg(feature = "tray")]
    pub(crate) fn latest(&self) -> anyhow::Result<Option<Clip>> {
        let volatile = self
            .volatile
            .lock()
            .map_err(|_| anyhow!("volatile history mutex poisoned"))?
            .iter()
            .filter(|clip| !is_expired(clip))
            .max_by_key(|clip| clip.meta.created_at)
            .cloned();
        let persistent = self
            .store
            .lock()
            .map_err(|_| anyhow!("history store mutex poisoned"))?
            .latest_by_recency()?;
        Ok(match (volatile, persistent) {
            (Some(volatile), Some(persistent))
                if persistent.meta.created_at > volatile.meta.created_at =>
            {
                Some(persistent)
            }
            (Some(volatile), _) => Some(volatile),
            (None, persistent) => persistent,
        })
    }

    pub(crate) fn maintain_idle(
        &self,
        background_work: bool,
        secret_ttl: Duration,
    ) -> anyhow::Result<Option<MaintenanceSummary>> {
        let volatile_expired = self.purge_expired_volatile()?;
        self.prune_expired_snapshot()?;
        let (summary, refreshed_clips, digest) = {
            let store = match self.store.try_lock() {
                Ok(store) => store,
                Err(std::sync::TryLockError::WouldBlock) => return Ok(None),
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err(anyhow!("history store mutex poisoned"));
                }
            };
            let expired = store.purge_expired()?;
            let clawback = store.clawback_sensitive(32, secret_ttl)?;
            let fingerprints = if background_work {
                store.backfill_fingerprints(32)?
            } else {
                0
            };
            let normalized_fingerprints = if background_work {
                store.backfill_normalized_fingerprints(32)?
            } else {
                0
            };
            let embeddings = if background_work {
                store.backfill_embeddings(32)?
            } else {
                0
            };
            let audit = store.audit_content_hashes(32)?;
            let fts_optimized = background_work && store.maintain_search_index(256)?;
            let blobs_collected = store.gc_blobs()?;
            let changed_visible_rows = volatile_expired > 0
                || expired > 0
                || clawback.reclassified > 0
                || audit.repaired > 0
                || audit.quarantined > 0;
            let refreshed_clips = changed_visible_rows
                .then(|| store.load_recent(self.snapshot_limit.load(Ordering::Relaxed)))
                .transpose()?;
            let digest = store.clipboard_health_digest()?;
            (
                MaintenanceSummary {
                    fingerprints,
                    normalized_fingerprints,
                    embeddings,
                    audited: audit.checked,
                    repaired: audit.repaired,
                    quarantined: audit.quarantined,
                    reclassified_sensitive: clawback.reclassified,
                    expired,
                    blobs_collected,
                    fts_optimized,
                },
                refreshed_clips,
                digest,
            )
        };

        let refreshed_clips = refreshed_clips
            .map(|clips| self.merge_volatile(clips, self.snapshot_limit.load(Ordering::Relaxed)))
            .transpose()?;
        let memory_only_clips = self.current_volatile_ids()?;
        let mut state = self
            .shared
            .lock()
            .map_err(|_| anyhow!("GUI state mutex poisoned"))?;
        if let Some(clips) = refreshed_clips {
            state.set_clips(clips);
        }
        state.memory_only_clips = memory_only_clips;
        state.health_digest = digest;
        Ok(Some(summary))
    }

    pub(crate) fn refresh_for_memory(&self, limit: usize) -> anyhow::Result<bool> {
        let limit = limit.max(1);
        let clips = {
            let store = match self.store.try_lock() {
                Ok(store) => store,
                Err(std::sync::TryLockError::WouldBlock) => return Ok(false),
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err(anyhow!("history store mutex poisoned"));
                }
            };
            store.load_recent(limit)?
        };
        let clips = self.merge_volatile(clips, limit)?;
        self.snapshot_limit.store(limit, Ordering::Relaxed);
        let memory_only_clips = self.current_volatile_ids()?;
        let mut state = self
            .shared
            .lock()
            .map_err(|_| anyhow!("GUI state mutex poisoned"))?;
        state.set_clips(clips);
        state.memory_only_clips = memory_only_clips;
        Ok(true)
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
            store.load_recent(self.snapshot_limit.load(Ordering::Relaxed))?
        };
        let limit = self.snapshot_limit.load(Ordering::Relaxed);
        let clips = self.merge_volatile(clips, limit)?;
        let memory_only_clips = self.current_volatile_ids()?;

        let mut state = self
            .shared
            .lock()
            .map_err(|_| anyhow!("GUI state mutex poisoned"))?;
        state.set_clips(clips);
        state.memory_only_clips = memory_only_clips;
        Ok(())
    }

    fn current_volatile_ids(&self) -> anyhow::Result<HashSet<ClipId>> {
        Ok(self
            .volatile
            .lock()
            .map_err(|_| anyhow!("volatile history mutex poisoned"))?
            .iter()
            .filter(|clip| !is_expired(clip))
            .map(|clip| clip.id)
            .collect())
    }

    fn purge_expired_volatile(&self) -> anyhow::Result<usize> {
        let mut volatile = self
            .volatile
            .lock()
            .map_err(|_| anyhow!("volatile history mutex poisoned"))?;
        let previous_len = volatile.len();
        volatile.retain(|clip| !is_expired(clip));
        Ok(previous_len - volatile.len())
    }

    fn prune_expired_snapshot(&self) -> anyhow::Result<()> {
        let memory_only_clips = self.current_volatile_ids()?;
        let mut state = self
            .shared
            .lock()
            .map_err(|_| anyhow!("GUI state mutex poisoned"))?;
        if state.clips.iter().any(is_expired) {
            let active = state
                .clips
                .iter()
                .filter(|clip| !is_expired(clip))
                .cloned()
                .collect::<Vec<_>>();
            state.set_clips(active);
        }
        state.memory_only_clips = memory_only_clips;
        Ok(())
    }

    fn refresh_snapshot(&self) -> anyhow::Result<()> {
        let limit = self.snapshot_limit.load(Ordering::Relaxed);
        let persistent = self
            .store
            .lock()
            .map_err(|_| anyhow!("history store mutex poisoned"))?
            .load_recent(limit)?;
        let clips = self.merge_volatile(persistent, limit)?;
        let memory_only_clips = self.current_volatile_ids()?;
        let mut state = self
            .shared
            .lock()
            .map_err(|_| anyhow!("GUI state mutex poisoned"))?;
        state.set_clips(clips);
        state.memory_only_clips = memory_only_clips;
        Ok(())
    }

    fn merge_volatile(&self, persistent: Vec<Clip>, limit: usize) -> anyhow::Result<Vec<Clip>> {
        let volatile = self
            .volatile
            .lock()
            .map_err(|_| anyhow!("volatile history mutex poisoned"))?
            .iter()
            .filter(|clip| !is_expired(clip))
            .cloned()
            .collect::<Vec<_>>();
        let pinned_end = persistent
            .iter()
            .position(|clip| !clip.pinned)
            .unwrap_or(persistent.len());
        let mut merged = Vec::with_capacity(persistent.len().saturating_add(volatile.len()));
        merged.extend_from_slice(&persistent[..pinned_end]);
        merged.extend(volatile);
        merged.extend_from_slice(&persistent[pinned_end..]);
        merged.truncate(limit.max(1));
        Ok(merged)
    }
}

fn is_expired(clip: &Clip) -> bool {
    clip.meta
        .expires_at
        .is_some_and(|expiry| expiry <= Utc::now())
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

        let summary = history
            .maintain_idle(true, std::time::Duration::from_secs(300))
            .unwrap()
            .unwrap();

        assert_eq!(summary.expired, 1);
        assert!(shared.lock().unwrap().clips.is_empty());
    }

    #[test]
    fn memory_snapshot_limit_survives_later_mutations() {
        let store = Store::open_in_memory().unwrap();
        for text in ["one", "two", "three"] {
            let flavors = vec![Flavor::inline("text/plain", text.as_bytes().to_vec())];
            let clip = Clip {
                id: ClipId::new(),
                content_hash: vbuff_core::content_hash_from_flavors(&flavors),
                flavors,
                meta: ClipMeta::now(ContentKind::Text, text.len() as u64, None),
                pinned: false,
                favorite: false,
            };
            store.insert(&clip).unwrap();
        }
        let initial = store.list(10).unwrap();
        let first_id = initial[0].id;
        let shared = Arc::new(Mutex::new(AppState::with_clips(initial)));
        let history = History::new(store, Arc::clone(&shared), 10);

        assert!(history.refresh_for_memory(1).unwrap());
        history.set_pinned(first_id, true).unwrap();

        assert_eq!(shared.lock().unwrap().clips.len(), 1);
    }

    #[test]
    fn memory_only_clip_never_enters_store_and_undo_restores_volatile_lane() {
        let flavors = vec![Flavor::inline("text/plain", b"123456".to_vec())];
        let mut meta = ClipMeta::now(ContentKind::Text, 6, None);
        meta.sensitive = true;
        meta.sync_eligible = false;
        meta.sensitivity_reason = Some(vbuff_types::SensitivityReason::OneTimePassword);
        meta.expires_at = Some(Utc::now() + Duration::seconds(60));
        let clip = Clip {
            id: ClipId::new(),
            content_hash: vbuff_core::content_hash_from_flavors(&flavors),
            flavors,
            meta,
            pinned: false,
            favorite: false,
        };
        let id = clip.id;
        let shared = Arc::new(Mutex::new(AppState::default()));
        let history = History::new(Store::open_in_memory().unwrap(), Arc::clone(&shared), 100);

        history.insert_volatile(clip.clone()).unwrap();
        assert!(
            history
                .store
                .lock()
                .unwrap()
                .get_clip(id)
                .unwrap()
                .is_none()
        );
        assert!(shared.lock().unwrap().memory_only_clips.contains(&id));
        assert!(history.set_pinned(id, true).is_err());
        assert!(history.set_session_protected(id, true).is_err());

        history.delete(id).unwrap();
        assert!(history.find(id).unwrap().is_none());
        history.restore(clip, 100).unwrap();
        assert!(history.find(id).unwrap().is_some());
        assert!(
            history
                .store
                .lock()
                .unwrap()
                .get_clip(id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn expired_payload_leaves_snapshot_even_when_store_is_busy() {
        let shared = std::sync::Arc::new(std::sync::Mutex::new(vbuff_gui::AppState::default()));
        let history = History::new(Store::open_in_memory().unwrap(), shared.clone(), 10);
        let flavors = vec![Flavor::inline("text/plain", b"short lived".to_vec())];
        let mut meta = ClipMeta::now(ContentKind::Text, 11, None);
        meta.sensitive = true;
        meta.expires_at = Some(Utc::now() + chrono::Duration::milliseconds(5));
        let clip = Clip {
            id: ClipId::new(),
            content_hash: vbuff_core::content_hash_from_flavors(&flavors),
            flavors,
            meta,
            pinned: false,
            favorite: false,
        };
        history.insert_volatile(clip).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));

        let _store_guard = history.store.lock().unwrap();
        assert!(
            history
                .maintain_idle(false, std::time::Duration::from_secs(60))
                .unwrap()
                .is_none()
        );
        let state = shared.lock().unwrap();
        assert!(state.clips.is_empty());
        assert!(state.memory_only_clips.is_empty());
    }
}
