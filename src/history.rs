//! Application-layer access to history and the GUI snapshot.
//!
//! The SQLite store remains the source of truth. This facade keeps mutex and
//! snapshot-refresh plumbing out of capture, tray, and command handling.

use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use vbuff_gui::SharedState;
use vbuff_store::Store;
use vbuff_types::{Clip, ClipId};

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
