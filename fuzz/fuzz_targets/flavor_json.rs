#![no_main]

use libfuzzer_sys::fuzz_target;
use vbuff_types::Flavor;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<Vec<Flavor>>(data);
});
