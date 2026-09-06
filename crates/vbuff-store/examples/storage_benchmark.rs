//! Reproducible disk benchmark; run each backend separately with --release.
//! Synthetic text only. Databases live in temporary directories and are removed.
use std::{hint::black_box, time::Instant};
use vbuff_core::content_hash_from_flavors;
use vbuff_store::Store;
use vbuff_types::{Clip, ClipId, ClipMeta, ContentKind, Flavor};

fn clip(index: usize) -> Clip {
    let text = format!(
        "clipboard item {index:06} category{} rust example заметка {}",
        index % 10,
        "payload ".repeat(56)
    );
    let flavors = vec![Flavor::inline("text/plain", text.as_bytes().to_vec())];
    Clip {
        id: ClipId::new(),
        content_hash: content_hash_from_flavors(&flavors),
        meta: ClipMeta::now(ContentKind::Text, text.len() as u64, None),
        flavors,
        pinned: false,
        favorite: false,
    }
}
fn report(rows: usize, operation: &str, mut samples: Vec<f64>) {
    let total: f64 = samples.iter().sum();
    samples.sort_by(f64::total_cmp);
    let percentile =
        |p: f64| samples[((samples.len() as f64 * p).ceil() as usize).saturating_sub(1)];
    println!(
        "{},{rows},{operation},{},{:.3},{:.3},{:.3}",
        vbuff_store::BACKEND,
        samples.len(),
        total,
        percentile(0.5),
        percentile(0.95)
    );
}
fn measure(mut operation: impl FnMut(), count: usize) -> Vec<f64> {
    (0..count)
        .map(|_| {
            let start = Instant::now();
            operation();
            start.elapsed().as_secs_f64() * 1000.0
        })
        .collect()
}
fn bytes(path: &std::path::Path) -> u64 {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                bytes(&path)
            } else {
                path.metadata().unwrap().len()
            }
        })
        .sum()
}
fn main() {
    println!("backend,rows,operation,samples,total_ms,p50_ms,p95_ms");
    for rows in [1_000, 10_000] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(if vbuff_store::BACKEND == "duckdb" {
            "history.duckdb"
        } else {
            "history.db"
        });
        let clips: Vec<_> = (0..rows).map(clip).collect();
        let store = Store::open(&path).unwrap();
        let mut index = 0;
        report(
            rows,
            "insert",
            measure(
                || {
                    black_box(store.insert(&clips[index]).unwrap());
                    index += 1;
                },
                rows,
            ),
        );
        assert_eq!(store.list(rows).unwrap().len(), rows);
        report(
            rows,
            "deduplicate",
            measure(
                || {
                    black_box(store.insert(&clips[rows / 2]).unwrap());
                },
                100,
            ),
        );
        // Warm these paths before measuring steady-state reads.
        black_box(store.list(50).unwrap());
        black_box(store.search("category3", 20).unwrap());
        report(
            rows,
            "list_50",
            measure(
                || {
                    let result = store.list(50).unwrap();
                    assert_eq!(result.len(), 50);
                    black_box(result);
                },
                100,
            ),
        );
        report(
            rows,
            "search_common",
            measure(
                || {
                    let result = store.search("category3", 20).unwrap();
                    assert_eq!(result.len(), 20);
                    black_box(result);
                },
                100,
            ),
        );
        report(
            rows,
            "search_missing",
            measure(
                || {
                    assert!(store.search("absentneedle", 20).unwrap().is_empty());
                },
                100,
            ),
        );
        report(
            rows,
            "recall_page_64",
            measure(
                || {
                    let (result, _) = store.recall_batch(None, 64).unwrap();
                    assert_eq!(result.len(), 64);
                    black_box(result);
                },
                100,
            ),
        );
        drop(store);
        report(
            rows,
            "reopen",
            measure(
                || {
                    black_box(Store::open(&path).unwrap());
                },
                10,
            ),
        );
        eprintln!(
            "backend={} rows={rows} disk_bytes={}",
            vbuff_store::BACKEND,
            bytes(directory.path())
        );
    }
}
