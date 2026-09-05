//! One cancellable recall worker. The mailbox holds only the latest request.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use crate::history::History;
use chrono::Utc;
use vbuff_core::recall::{RecallSearchContext, parse_natural_query, search_recall};
use vbuff_gui::experience::HistoryScope;
use vbuff_gui::{HistorySearchResults, SharedState};
use vbuff_types::Clip;

const RESULT_BYTES: usize = 32 * 1024 * 1024;

struct Request {
    query: String,
    scope: HistoryScope,
    revision: u64,
    generation: u64,
    ctx: egui::Context,
}

#[derive(Default)]
struct Mailbox {
    request: Option<Request>,
    stopped: bool,
}

pub(crate) struct HistorySearch {
    mailbox: Arc<(Mutex<Mailbox>, Condvar)>,
    generation: Arc<AtomicU64>,
    last: Option<(String, HistoryScope, u64)>,
}

impl HistorySearch {
    pub(crate) fn new(history: History, shared: SharedState) -> Self {
        let mailbox = Arc::new((Mutex::new(Mailbox::default()), Condvar::new()));
        let generation = Arc::new(AtomicU64::new(0));
        let worker_mailbox = Arc::clone(&mailbox);
        let worker_generation = Arc::clone(&generation);
        std::thread::spawn(move || {
            loop {
                let request = {
                    let (lock, wake) = &*worker_mailbox;
                    let Ok(mut state) = lock.lock() else { return };
                    while state.request.is_none() && !state.stopped {
                        let Ok(next) = wake.wait(state) else { return };
                        state = next;
                    }
                    if state.stopped {
                        return;
                    }
                    state.request.take().expect("mailbox is ready")
                };
                let current = || worker_generation.load(Ordering::Acquire) == request.generation;
                let result = collect(&history, &request.query, &request.scope, current);
                if !current() {
                    continue;
                }
                if let Ok(mut state) = shared.lock()
                    && state.revision == request.revision
                    && current()
                {
                    let (clips, total, failed) = match result {
                        Ok(Some((clips, total))) => (clips, total, false),
                        Ok(None) => continue,
                        Err(_) => (Vec::new(), 0, true),
                    };
                    state.history_search = Some(HistorySearchResults {
                        query: request.query,
                        scope: request.scope,
                        history_revision: request.revision,
                        clips: clips.into(),
                        total,
                        failed,
                    });
                }
                request.ctx.request_repaint();
            }
        });
        Self {
            mailbox,
            generation,
            last: None,
        }
    }

    pub(crate) fn update(
        &mut self,
        intent: Option<(String, HistoryScope)>,
        revision: u64,
        ctx: &egui::Context,
    ) {
        let next = intent.map(|(query, scope)| (query, scope, revision));
        if self.last == next {
            return;
        }
        self.last = next.clone();
        let generation = self
            .generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        let (lock, wake) = &*self.mailbox;
        if let Ok(mut mailbox) = lock.lock() {
            mailbox.request = next.map(|(query, scope, revision)| Request {
                query,
                scope,
                revision,
                generation,
                ctx: ctx.clone(),
            });
            wake.notify_one();
        }
    }
}

impl Drop for HistorySearch {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        let (lock, wake) = &*self.mailbox;
        if let Ok(mut state) = lock.lock() {
            state.stopped = true;
            state.request = None;
            wake.notify_one();
        }
    }
}

fn collect(
    history: &History,
    raw: &str,
    scope: &HistoryScope,
    current: impl Fn() -> bool,
) -> anyhow::Result<Option<(Vec<Clip>, usize)>> {
    let now = Utc::now();
    let Ok(query) = parse_natural_query(raw, now) else {
        return Ok(Some((Vec::new(), 0)));
    };
    let tags = history.tag_snapshot()?;
    let mut winners: Vec<(i64, Clip)> = Vec::new();
    let mut total = 0;
    let mut cursor = None;
    let mut batch = history.volatile_snapshot()?;
    loop {
        if !current() {
            return Ok(None);
        }
        let mut recall_tags = vbuff_core::recall::ClipTags::default();
        if let Some(label) = &query.tag {
            let variants = vbuff_core::recall::layout_variants(label);
            for tag in tags.tags.iter().filter(|t| variants.contains(&t.name)) {
                for clip in &batch {
                    if tag.clips.contains(&clip.id) {
                        recall_tags.add_tag(clip.id, label);
                    }
                }
            }
        }
        for hit in search_recall(
            &batch,
            &query,
            RecallSearchContext {
                tags: Some(&recall_tags),
                ..Default::default()
            },
        ) {
            if scope.matches_tags(hit.clip, &tags)
                && hit.clip.meta.expires_at.is_none_or(|expiry| expiry > now)
            {
                total += 1;
                winners.push((hit.score, hit.clip.clone()));
            }
        }
        winners.sort_by(|(a_score, a), (b_score, b)| {
            b.pinned
                .cmp(&a.pinned)
                .then_with(|| b_score.cmp(a_score))
                .then_with(|| b.meta.updated_at.cmp(&a.meta.updated_at))
        });
        winners.truncate(history.recall_result_limit());
        let mut bytes = 0_usize;
        let keep = winners
            .iter()
            .enumerate()
            .take_while(|(index, (_, clip))| {
                bytes = bytes.saturating_add(
                    clip.flavors
                        .iter()
                        .map(|f| f.body.inline_bytes().map_or(0, |b| b.len()))
                        .sum::<usize>(),
                );
                // Keep at least the best match, even for a single oversized clip.
                *index == 0 || bytes <= RESULT_BYTES
            })
            .count();
        winners.truncate(keep);
        if !current() {
            return Ok(None);
        }
        let (next, next_cursor) = history.recall_batch(cursor, &query.text)?;
        if next.is_empty() {
            break;
        }
        batch = next;
        cursor = next_cursor;
    }
    Ok(Some((
        winners.into_iter().map(|(_, clip)| clip).collect(),
        total,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_gui::AppState;
    use vbuff_store::Store;
    use vbuff_types::{ClipId, ClipMeta, ContentKind, Flavor};

    fn clip(text: &str) -> Clip {
        let flavors = vec![Flavor::inline("text/plain", text.as_bytes().to_vec())];
        Clip {
            id: ClipId::new(),
            content_hash: vbuff_core::content_hash_from_flavors(&flavors),
            meta: ClipMeta::now(ContentKind::Text, text.len() as u64, None),
            flavors,
            pinned: false,
            favorite: false,
        }
    }

    #[test]
    fn wrong_layout_search_reaches_old_rows_in_both_directions() {
        let store = Store::open_in_memory().unwrap();
        let russian = clip("привет мир");
        let english = clip("hello world");
        store.insert(&russian).unwrap();
        store.insert(&english).unwrap();
        store.insert(&clip("most recent unrelated")).unwrap();
        let history = History::new(store, Arc::new(Mutex::new(AppState::default())), 1);
        for (query, expected) in [("ghbdtn", russian.id), ("руддщ", english.id)] {
            let (matches, total) = collect(&history, query, &HistoryScope::All, || true)
                .unwrap()
                .unwrap();
            assert_eq!(total, 1);
            assert_eq!(matches[0].id, expected);
        }
    }

    #[test]
    fn tag_filter_searches_beyond_recent_snapshot() {
        let store = Store::open_in_memory().unwrap();
        let old = clip("old tagged entry");
        store.insert(&old).unwrap();
        store
            .edit_tags(&vbuff_types::TagCommand::Save {
                id: None,
                name: "work".into(),
                color: [0; 3],
            })
            .unwrap();
        let tag = store.tag_snapshot().unwrap().tags[0].id.clone();
        store
            .edit_tags(&vbuff_types::TagCommand::Assign {
                clips: vec![old.id],
                tag: tag.clone(),
                assigned: true,
            })
            .unwrap();
        store.insert(&clip("new untagged entry")).unwrap();
        let history = History::new(store, Arc::new(Mutex::new(AppState::default())), 1);
        let scope = HistoryScope::Tagged {
            base: Box::new(HistoryScope::Kind(ContentKind::Text)),
            ids: vec![tag],
            all: true,
        };
        let (matches, total) = collect(&history, "", &scope, || true).unwrap().unwrap();
        assert_eq!(total, 1);
        assert_eq!(matches[0].id, old.id);
        let (matches, total) = collect(&history, "tag:цщкл", &HistoryScope::All, || true)
            .unwrap()
            .unwrap();
        assert_eq!(total, 1);
        assert_eq!(matches[0].id, old.id);
    }

    #[test]
    #[ignore = "manual release-mode full-history latency baseline"]
    fn full_history_latency_baseline() {
        for rows in [1_000, 10_000, 100_000] {
            let store = Store::open_in_memory().unwrap();
            store.insert(&clip("elephant unique old record")).unwrap();
            for index in 1..rows {
                store
                    .insert(&clip(&format!("ordinary clipboard record {index}")))
                    .unwrap();
            }
            let history = History::new(store, Arc::new(Mutex::new(AppState::default())), 1_000);
            let started = std::time::Instant::now();
            let (matches, count) = collect(&history, "elephant", &HistoryScope::All, || true)
                .unwrap()
                .unwrap();
            assert_eq!(count, 1);
            assert_eq!(matches.len(), 1);
            println!(
                "metric=full_history_recall rows={rows} elapsed_ms={}",
                started.elapsed().as_millis()
            );
        }
    }

    #[test]
    fn result_limit_tracks_memory_policy_without_hiding_total() {
        let store = Store::open_in_memory().unwrap();
        for index in 0..70 {
            store.insert(&clip(&format!("elephant {index}"))).unwrap();
        }
        let history = History::new(store, Arc::new(Mutex::new(AppState::default())), 2);
        let (matches, total) = collect(&history, "elephant", &HistoryScope::All, || true)
            .unwrap()
            .unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(total, 70);
    }

    #[test]
    fn archived_rows_never_reach_recall() {
        let store = Store::open_in_memory().unwrap();
        let archived = clip("hidden elephant");
        store.insert(&archived).unwrap();
        store.set_archived(archived.id, true).unwrap();
        let history = History::new(store, Arc::new(Mutex::new(AppState::default())), 1);
        assert_eq!(
            collect(&history, "elephant", &HistoryScope::All, || true)
                .unwrap()
                .unwrap()
                .1,
            0
        );
    }

    #[test]
    fn worker_publishes_only_latest_query_and_revision() {
        let shared = Arc::new(Mutex::new(AppState::default()));
        let history = History::new(Store::open_in_memory().unwrap(), shared.clone(), 2);
        history.insert(&clip("elephant"), 10).unwrap();
        let revision = shared.lock().unwrap().revision;
        let mut worker = HistorySearch::new(history.clone(), shared.clone());
        let ctx = egui::Context::default();
        worker.update(Some(("elephant".into(), HistoryScope::All)), revision, &ctx);
        worker.update(Some(("giraffe".into(), HistoryScope::All)), revision, &ctx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if shared
                .lock()
                .unwrap()
                .history_search
                .as_ref()
                .is_some_and(|r| r.query == "giraffe")
            {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            shared
                .lock()
                .unwrap()
                .history_search
                .as_ref()
                .unwrap()
                .total,
            0
        );
        history.insert(&clip("giraffe"), 10).unwrap();
        assert!(shared.lock().unwrap().history_search.is_none());
        let revision = shared.lock().unwrap().revision;
        worker.update(Some(("giraffe".into(), HistoryScope::All)), revision, &ctx);
        loop {
            if shared
                .lock()
                .unwrap()
                .history_search
                .as_ref()
                .is_some_and(|r| r.total == 1 && r.history_revision == revision)
            {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn searches_beyond_snapshot_with_same_typo_and_filter_semantics() {
        let store = Store::open_in_memory().unwrap();
        let old = clip("elephant");
        store.insert(&old).unwrap();
        for n in 0..130 {
            store.insert(&clip(&format!("new row {n}"))).unwrap();
        }
        let state = Arc::new(Mutex::new(AppState::default()));
        let history = History::new(store, state.clone(), 2);
        history.refresh_for_memory(2).unwrap();
        assert!(!state.lock().unwrap().clips.iter().any(|c| c.id == old.id));
        let (matches, count) = collect(&history, "elephnt kind:text", &HistoryScope::All, || true)
            .unwrap()
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(matches[0].id, old.id);
        assert!(
            collect(&history, "kind:invalid", &HistoryScope::All, || true)
                .unwrap()
                .unwrap()
                .0
                .is_empty()
        );
        assert!(
            collect(&history, "elephant", &HistoryScope::All, || false)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn secret_payload_is_not_searchable_but_volatile_metadata_is() {
        let history = History::new(
            Store::open_in_memory().unwrap(),
            Arc::new(Mutex::new(AppState::default())),
            2,
        );
        let mut secret = clip("private-needle");
        secret.meta.sensitive = true;
        secret.meta.source_app = Some("Editor".into());
        secret.meta.expires_at = Some(Utc::now() + chrono::Duration::minutes(1));
        history.insert_volatile(secret).unwrap();
        assert_eq!(
            collect(&history, "private-needle", &HistoryScope::All, || true)
                .unwrap()
                .unwrap()
                .1,
            0
        );
        assert_eq!(
            collect(&history, "app:editor", &HistoryScope::All, || true)
                .unwrap()
                .unwrap()
                .1,
            1
        );
    }
}
