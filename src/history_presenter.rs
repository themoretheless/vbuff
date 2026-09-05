//! GUI adapter for application history events. The history service never locks a window.
use crate::history::{HistoryEvent, HistoryEvents};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

impl From<vbuff_gui::SharedState> for HistoryEvents {
    fn from(shared: vbuff_gui::SharedState) -> Self {
        let latest = AtomicU64::new(0);
        let latest_tags = AtomicU64::new(0);
        Self(Arc::new(move |event| {
            let mut state = shared
                .lock()
                .map_err(|_| anyhow::anyhow!("GUI state mutex poisoned"))?;
            let version = match &event {
                HistoryEvent::Snapshot { version, .. } => Some(*version),
                HistoryEvent::Maintenance { version, .. } => *version,
                _ => None,
            };
            if let Some(version) = version {
                if version < latest.load(Ordering::Relaxed) {
                    return Ok(());
                }
                latest.store(version, Ordering::Relaxed);
            }
            let active = |clips: Vec<vbuff_types::Clip>| {
                let now = chrono::Utc::now();
                clips
                    .into_iter()
                    .filter(|clip| clip.meta.expires_at.is_none_or(|expiry| expiry > now))
                    .collect()
            };
            match event {
                HistoryEvent::Tags { version, tags } => {
                    if version >= latest_tags.load(Ordering::Relaxed) {
                        latest_tags.store(version, Ordering::Relaxed);
                        if *state.tags != tags {
                            state.tags = Arc::new(tags);
                            state.revision = state.revision.wrapping_add(1);
                        }
                    }
                }
                HistoryEvent::Snapshot {
                    clips,
                    memory_only_clips,
                    ..
                } => {
                    state.set_clips(active(clips));
                    state.memory_only_clips = memory_only_clips;
                }
                HistoryEvent::Maintenance {
                    clips,
                    memory_only_clips,
                    digest,
                    ..
                } => {
                    if let Some(clips) = clips {
                        state.set_clips(active(clips));
                    }
                    state.memory_only_clips = memory_only_clips;
                    state.health_digest = digest;
                }
                HistoryEvent::Protection { id, protected } => {
                    if protected {
                        state.session_protected.insert(id);
                    } else {
                        state.session_protected.remove(&id);
                    }
                    state.revision = state.revision.wrapping_add(1);
                }
                HistoryEvent::PruneExpired { memory_only_clips } => {
                    let now = chrono::Utc::now();
                    let expired = |clip: &vbuff_types::Clip| {
                        clip.meta.expires_at.is_some_and(|expiry| expiry <= now)
                    };
                    if state
                        .history_search
                        .as_ref()
                        .is_some_and(|result| result.clips.iter().any(expired))
                    {
                        state.history_search = None;
                        state.revision = state.revision.wrapping_add(1);
                    }
                    if state.clips.iter().any(expired) {
                        let clips = state
                            .clips
                            .iter()
                            .filter(|clip| !expired(clip))
                            .cloned()
                            .collect();
                        state.set_clips(active(clips));
                    }
                    state.memory_only_clips = memory_only_clips;
                }
            }
            Ok(())
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delayed_snapshot_cannot_replace_newer_history_or_reintroduce_expiry() {
        let shared = Arc::new(std::sync::Mutex::new(vbuff_gui::AppState::default()));
        let events = HistoryEvents::from(shared.clone());
        let mut clip = vbuff_types::Clip {
            id: vbuff_types::ClipId::new(),
            flavors: vec![vbuff_types::Flavor::inline("text/plain", b"old".to_vec())],
            content_hash: [0; 32],
            meta: vbuff_types::ClipMeta::now(vbuff_types::ContentKind::Text, 3, None),
            pinned: false,
            favorite: false,
        };
        events.0(HistoryEvent::Snapshot {
            version: 2,
            clips: vec![],
            memory_only_clips: Default::default(),
        })
        .unwrap();
        let revision = shared.lock().unwrap().revision;
        events.0(HistoryEvent::Snapshot {
            version: 1,
            clips: vec![clip.clone()],
            memory_only_clips: Default::default(),
        })
        .unwrap();
        assert_eq!(shared.lock().unwrap().revision, revision);
        assert!(shared.lock().unwrap().clips.is_empty());
        clip.meta.expires_at = Some(chrono::Utc::now() - chrono::Duration::seconds(1));
        events.0(HistoryEvent::Snapshot {
            version: 3,
            clips: vec![clip],
            memory_only_clips: Default::default(),
        })
        .unwrap();
        assert!(shared.lock().unwrap().clips.is_empty());
    }
}
