use std::hint::black_box;
use std::time::{Duration, Instant};

use vbuff_core::{content_hash_from_flavors, detect_kind};
use vbuff_types::Flavor;

#[test]
#[ignore = "release-mode CI performance budget"]
fn capture_classification_and_hashing_stay_inside_budget() {
    let mut bytes = vec![b'a'; 16 * 1024];
    let started = Instant::now();
    for iteration in 0..10_000_u32 {
        bytes[0] = (iteration % 251) as u8;
        let flavors = [Flavor::inline("text/plain", bytes.clone())];
        black_box(content_hash_from_flavors(&flavors));
        black_box(detect_kind(&flavors));
    }
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "core capture hot path exceeded 3 seconds: {:?}",
        started.elapsed()
    );
}
