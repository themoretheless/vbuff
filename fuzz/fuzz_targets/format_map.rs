#![no_main]

use libfuzzer_sys::fuzz_target;
use vbuff_platform::{FormatFamily, canonical_format};

fuzz_target!(|data: &[u8]| {
    let Some((&selector, bytes)) = data.split_first() else {
        return;
    };
    let Ok(value) = std::str::from_utf8(bytes) else {
        return;
    };
    let family = match selector % 3 {
        0 => FormatFamily::MacUti,
        1 => FormatFamily::Windows,
        _ => FormatFamily::Mime,
    };
    let _ = canonical_format(family, value);
});
