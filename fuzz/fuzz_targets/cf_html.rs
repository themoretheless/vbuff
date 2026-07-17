#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(parsed) = vbuff_platform::parse_cf_html(data) {
        assert!(parsed.fragment.len() <= parsed.html.len());
        assert!(parsed.html.len() <= data.len());
    }
});
