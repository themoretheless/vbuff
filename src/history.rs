//! Application-layer access to history and the GUI snapshot.
//!
//! The DuckDB store remains the source of truth. This facade keeps mutex and
//! snapshot-refresh plumbing out of capture, tray, and command handling.

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::anyhow;
use chrono::Utc;
use vbuff_core::capture::CaptureOutcome;

use vbuff_store::Store;
use vbuff_types::{Clip, ClipId};

/// Domain events contain no window state, toolkit handles, or rendering commands.
pub(crate) enum HistoryEvent {
    Tags {
        version: u64,
        tags: vbuff_types::TagSnapshot,
    },
    Snapshot {
        version: u64,
        clips: Vec<Clip>,
        memory_only_clips: HashSet<ClipId>,
    },
    Maintenance {
        version: Option<u64>,
        clips: Option<Vec<Clip>>,
        memory_only_clips: HashSet<ClipId>,
        digest: vbuff_types::ClipboardHealthDigest,
    },
    Protection {
        id: ClipId,
        protected: bool,
    },
    PruneExpired {
        memory_only_clips: HashSet<ClipId>,
    },
}

#[derive(Clone)]
pub(crate) struct HistoryEvents(
    pub(crate) Arc<dyn Fn(HistoryEvent) -> anyhow::Result<()> + Send + Sync>,
);
impl HistoryEvents {
    fn send(&self, event: HistoryEvent) -> anyhow::Result<()> {
        (self.0)(event)
    }
}

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
    pub wal_scrubbed: bool,
}

/// Cloneable history handle shared by the capture and UI threads.
#[derive(Clone)]
pub(crate) struct History {
    store: Arc<crate::store_owner::StoreOwner>,
    volatile: Arc<Mutex<Vec<Clip>>>,
    volatile_origins: Arc<Mutex<VecDeque<ClipId>>>,
    events: HistoryEvents,
    snapshot_version: Arc<AtomicU64>,
    snapshot_limit: Arc<AtomicUsize>,
    deleted_original: Arc<Mutex<Option<(Clip, std::time::Instant)>>>,
}

impl History {
    pub(crate) fn new(
        store: Store,
        events: impl Into<HistoryEvents>,
        snapshot_limit: usize,
    ) -> Self {
        Self {
            store: Arc::new(crate::store_owner::StoreOwner::new(store)),
            volatile: Arc::new(Mutex::new(Vec::new())),
            volatile_origins: Arc::new(Mutex::new(VecDeque::new())),
            events: events.into(),
            snapshot_version: Arc::new(AtomicU64::new(0)),
            snapshot_limit: Arc::new(AtomicUsize::new(snapshot_limit.max(1))),
            deleted_original: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn tag_snapshot(&self) -> anyhow::Result<vbuff_types::TagSnapshot> {
        self.store.execute(|store| Ok(store.tag_snapshot()?))
    }
    pub(crate) fn refresh_tags(&self) -> anyhow::Result<()> {
        let versions = self.snapshot_version.clone();
        let (tags, version) = self.store.execute(move |store| {
            Ok((
                store.tag_snapshot()?,
                versions.fetch_add(1, Ordering::Relaxed) + 1,
            ))
        })?;
        self.events.send(HistoryEvent::Tags { version, tags })
    }
    pub(crate) fn edit_tags(&self, command: vbuff_types::TagCommand) -> anyhow::Result<()> {
        self.store
            .execute(move |store| Ok(store.edit_tags(&command)?))?;
        self.refresh_tags()
    }

    /// Insert a captured clip, enforce retention, and publish a fresh snapshot.
    pub(crate) fn insert(&self, clip: &Clip, max_history: usize) -> anyhow::Result<()> {
        let clip = clip.clone();
        self.mutate_and_refresh(move |store| {
            store.insert(&clip)?;
            store.enforce_cap(max_history)?;
            Ok(())
        })
    }

    /// Publish a short-lived clip without writing its payload to the database.
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
        let clips = clips.to_vec();
        self.mutate_and_refresh(move |store| {
            for clip in &clips {
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
            .execute(move |store| Ok(store.record_capture_outcome(outcome, count)?))?;
        Ok(())
    }

    pub(crate) fn set_ttl(&self, id: ClipId, seconds: Option<u64>) -> anyhow::Result<()> {
        if self.is_memory_only(id)? {
            return Err(anyhow!(
                "Memory-only expiry is controlled by privacy policy"
            ));
        }
        self.mutate_and_refresh(move |store| store.set_ttl(id, seconds))
    }

    pub(crate) fn set_pinned(&self, id: ClipId, pinned: bool) -> anyhow::Result<()> {
        if self.is_memory_only(id)? {
            return Err(anyhow!("memory-only clips cannot be pinned"));
        }
        self.mutate_and_refresh(move |store| store.set_pinned(id, pinned))
    }

    pub(crate) fn set_session_protected(&self, id: ClipId, protected: bool) -> anyhow::Result<()> {
        if self.is_memory_only(id)? {
            return Err(anyhow!(
                "memory-only clips cannot receive session protection"
            ));
        }
        self.store
            .execute(move |store| Ok(store.set_session_protected(id, protected)?))?;
        self.events
            .send(HistoryEvent::Protection { id, protected })?;
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
        let original = self.find(id)?;
        self.mutate_and_refresh(move |store| store.delete(id))?;
        *self
            .deleted_original
            .lock()
            .map_err(|_| anyhow!("undo mutex poisoned"))? =
            original.map(|clip| (clip, std::time::Instant::now()));
        self.events.send(HistoryEvent::Protection {
            id,
            protected: false,
        })?;
        Ok(())
    }

    /// Clear non-pinned history. The command name is shared across all surfaces.
    pub(crate) fn clear_history(&self) -> anyhow::Result<()> {
        *self
            .deleted_original
            .lock()
            .map_err(|_| anyhow!("undo mutex poisoned"))? = None;
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
        self.store.execute(move |store| Ok(store.get_clip(id)?))
    }

    pub(crate) fn query(
        &self,
        request: crate::single_instance::HistoryRequest,
    ) -> anyhow::Result<String> {
        self.store.execute(move |store| match request {
            crate::single_instance::HistoryRequest::Ask { query, limit } => {
                crate::ask::query_store(store, &query, limit)
            }
            crate::single_instance::HistoryRequest::Doctor => {
                Ok(serde_json::to_string(&store.doctor()?)?)
            }
        })
    }

    pub(crate) fn recall_result_limit(&self) -> usize {
        self.snapshot_limit.load(Ordering::Relaxed).clamp(1, 1_000)
    }

    pub(crate) fn recall_batch(
        &self,
        cursor: Option<i64>,
        text: &str,
    ) -> anyhow::Result<(Vec<Clip>, Option<i64>)> {
        let text = text.to_owned();
        self.store
            .execute(move |store| Ok(store.recall_candidates_batch(cursor, 64, &text)?))
    }

    pub(crate) fn volatile_snapshot(&self) -> anyhow::Result<Vec<Clip>> {
        Ok(self
            .volatile
            .lock()
            .map_err(|_| anyhow!("volatile history mutex poisoned"))?
            .iter()
            .filter(|clip| !is_expired(clip))
            .cloned()
            .collect())
    }

    pub(crate) fn is_memory_only(&self, id: ClipId) -> anyhow::Result<bool> {
        Ok(self
            .volatile_origins
            .lock()
            .map_err(|_| anyhow!("volatile origin mutex poisoned"))?
            .contains(&id))
    }

    pub(crate) fn restore(&self, clip: Clip, max_history: usize) -> anyhow::Result<()> {
        let mut original = self
            .deleted_original
            .lock()
            .map_err(|_| anyhow!("undo mutex poisoned"))?;
        let clip = if original.as_ref().is_some_and(|(saved, at)| {
            saved.id == clip.id && at.elapsed() <= Duration::from_secs(5)
        }) {
            original.take().expect("matched undo original").0
        } else {
            anyhow::ensure!(
                !clip
                    .flavors
                    .iter()
                    .any(|flavor| matches!(flavor.body, vbuff_types::Body::Spilled { .. })),
                "undo original is no longer available"
            );
            clip
        };
        drop(original);
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
        let persistent = self.store.execute(|store| Ok(store.latest_by_recency()?))?;
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
        if let Ok(mut original) = self.deleted_original.lock()
            && original
                .as_ref()
                .is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(5))
        {
            *original = None;
        }
        self.prune_expired_snapshot()?;
        let limit = self.snapshot_limit.load(Ordering::Relaxed);
        let versions = self.snapshot_version.clone();
        let Some((summary, refreshed_clips, digest, version)) =
            self.store.try_execute(move |store| {
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
                let wal_scrubbed = store.scrub_wal_if_dirty()?;
                let changed_visible_rows = volatile_expired > 0
                    || expired > 0
                    || clawback.reclassified > 0
                    || audit.repaired > 0
                    || audit.quarantined > 0;
                let refreshed_clips = changed_visible_rows
                    .then(|| store.load_recent_for_ui(limit))
                    .transpose()?;
                let digest = store.clipboard_health_digest()?;
                Ok((
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
                        wal_scrubbed,
                    },
                    refreshed_clips,
                    digest,
                    changed_visible_rows.then(|| versions.fetch_add(1, Ordering::Relaxed) + 1),
                ))
            })?
        else {
            return Ok(None);
        };

        let refreshed_clips = refreshed_clips
            .map(|clips| self.merge_volatile(clips, self.snapshot_limit.load(Ordering::Relaxed)))
            .transpose()?;
        let memory_only_clips = self.current_volatile_ids()?;
        self.events.send(HistoryEvent::Maintenance {
            version,
            clips: refreshed_clips,
            memory_only_clips,
            digest,
        })?;
        self.refresh_tags()?;
        Ok(Some(summary))
    }

    /// Best-effort WAL scrub for process shutdown.
    ///
    /// Never fails the exit path: a poisoned mutex or a real SQLite error is
    /// logged with `tracing::warn` and swallowed. A scrub skipped here leaves
    /// the dirty marker in place, and the next launch's idle maintenance
    /// retries it.
    pub(crate) fn flush_for_shutdown(&self) {
        self.store.shutdown();
    }

    pub(crate) fn refresh_for_memory(&self, limit: usize) -> anyhow::Result<bool> {
        let limit = limit.max(1);
        let versions = self.snapshot_version.clone();
        let Some((clips, version)) = self.store.try_execute(move |store| {
            Ok((
                store.load_recent_for_ui(limit)?,
                versions.fetch_add(1, Ordering::Relaxed) + 1,
            ))
        })?
        else {
            return Ok(false);
        };
        let clips = self.merge_volatile(clips, limit)?;
        self.snapshot_limit.store(limit, Ordering::Relaxed);
        let memory_only_clips = self.current_volatile_ids()?;
        self.events.send(HistoryEvent::Snapshot {
            version,
            clips,
            memory_only_clips,
        })?;
        Ok(true)
    }

    fn mutate_and_refresh(
        &self,
        mutation: impl FnOnce(&Store) -> vbuff_store::Result<()> + Send + 'static,
    ) -> anyhow::Result<()> {
        let limit = self.snapshot_limit.load(Ordering::Relaxed);
        let versions = self.snapshot_version.clone();
        let (clips, version) = self.store.execute(move |store| {
            mutation(store)?;
            Ok((
                store.load_recent_for_ui(limit)?,
                versions.fetch_add(1, Ordering::Relaxed) + 1,
            ))
        })?;
        let clips = self.merge_volatile(clips, limit)?;
        let memory_only_clips = self.current_volatile_ids()?;

        self.events.send(HistoryEvent::Snapshot {
            version,
            clips,
            memory_only_clips,
        })?;
        self.refresh_tags()
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
        self.events
            .send(HistoryEvent::PruneExpired { memory_only_clips })?;
        Ok(())
    }

    fn refresh_snapshot(&self) -> anyhow::Result<()> {
        let limit = self.snapshot_limit.load(Ordering::Relaxed);
        let versions = self.snapshot_version.clone();
        let (persistent, version) = self.store.execute(move |store| {
            Ok((
                store.load_recent_for_ui(limit)?,
                versions.fetch_add(1, Ordering::Relaxed) + 1,
            ))
        })?;
        let clips = self.merge_volatile(persistent, limit)?;
        let memory_only_clips = self.current_volatile_ids()?;
        self.events.send(HistoryEvent::Snapshot {
            version,
            clips,
            memory_only_clips,
        })?;
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
    fn large_image_snapshot_defers_bytes_and_delete_undo_survives_blob_collection() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(&directory.path().join("history.db")).unwrap();
        let pixels = vec![127; 300 * 300 * 4];
        let flavors = vec![Flavor::inline(
            "image/x-vbuff-rgba;width=300;height=300",
            pixels,
        )];
        let clip = Clip {
            id: ClipId::new(),
            content_hash: vbuff_core::content_hash_from_flavors(&flavors),
            meta: ClipMeta::now(ContentKind::Image, 360_000, None),
            flavors,
            pinned: false,
            favorite: false,
        };
        let shared = Arc::new(Mutex::new(AppState::default()));
        let history = History::new(store, shared.clone(), 10);
        history.insert(&clip, 10).unwrap();
        let summary = shared.lock().unwrap().clips[0].clone();
        assert!(matches!(
            summary.flavors[0].body,
            vbuff_types::Body::Spilled { .. }
        ));
        assert_eq!(
            history.find(clip.id).unwrap().unwrap().flavors,
            clip.flavors
        );
        history.delete(clip.id).unwrap();
        history
            .store
            .execute(|store| Ok(store.gc_blobs()?))
            .unwrap();
        history.restore(summary, 10).unwrap();
        assert_eq!(
            history.find(clip.id).unwrap().unwrap().flavors,
            clip.flavors
        );
    }

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
        // The purge marked the WAL dirty, and maintain_idle ran the deferred
        // scrub in the same pass.
        assert!(summary.wal_scrubbed);
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
                .execute(move |store| Ok(store.get_clip(id)?))
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
                .execute(move |store| Ok(store.get_clip(id)?))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn expired_search_only_payload_is_released_while_store_is_busy() {
        let shared = Arc::new(Mutex::new(AppState::default()));
        let history = History::new(Store::open_in_memory().unwrap(), shared.clone(), 1);
        let flavors = vec![Flavor::inline(
            "text/plain",
            b"expired search secret".to_vec(),
        )];
        let mut meta = ClipMeta::now(ContentKind::Text, 21, None);
        meta.sensitive = true;
        meta.expires_at = Some(Utc::now() - chrono::Duration::seconds(1));
        let clip = Clip {
            id: ClipId::new(),
            content_hash: vbuff_core::content_hash_from_flavors(&flavors),
            flavors,
            meta,
            pinned: false,
            favorite: false,
        };
        shared.lock().unwrap().history_search = Some(vbuff_gui::HistorySearchResults {
            query: "kind:text".into(),
            scope: vbuff_gui::experience::HistoryScope::All,
            history_revision: 0,
            clips: vec![clip].into(),
            total: 1,
            failed: false,
        });
        let _busy = history.store.hold_busy();
        history
            .maintain_idle(false, std::time::Duration::from_secs(60))
            .unwrap();
        let state = shared.lock().unwrap();
        assert!(state.history_search.is_none());
        assert_eq!(state.revision, 1);
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

        let _store_guard = history.store.hold_busy();
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
