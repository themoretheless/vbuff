use std::hint::black_box;
use std::time::{Duration, Instant};

use vbuff_core::content_hash_from_flavors;
use vbuff_store::Store;
use vbuff_types::{Clip, ClipId, ClipMeta, ContentKind, Flavor};

fn clip(index: usize) -> Clip {
    let text = format!("performance searchable clipboard row {index}");
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

#[test]
#[ignore = "release-mode CI performance budget"]
fn thousand_row_insert_and_search_stay_inside_budget() {
    let store = Store::open_in_memory().unwrap();
    let insert_started = Instant::now();
    for index in 0..1_000 {
        store.insert(&clip(index)).unwrap();
    }
    let insert_elapsed = insert_started.elapsed();
    println!(
        "metric=store_insert rows=1000 elapsed_ms={} budget_ms=15000",
        insert_elapsed.as_millis()
    );
    assert!(
        insert_elapsed < Duration::from_secs(15),
        "1k inserts exceeded 15 seconds: {insert_elapsed:?}"
    );

    let search_started = Instant::now();
    for index in 0..100 {
        black_box(store.search(&format!("row {}", index % 10), 20).unwrap());
    }
    let search_elapsed = search_started.elapsed();
    println!(
        "metric=store_search queries=100 elapsed_ms={} budget_ms=5000",
        search_elapsed.as_millis()
    );
    assert!(
        search_elapsed < Duration::from_secs(5),
        "100 searches exceeded 5 seconds: {search_elapsed:?}"
    );
}
