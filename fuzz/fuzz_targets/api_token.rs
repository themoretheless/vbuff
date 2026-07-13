#![no_main]

use libfuzzer_sys::fuzz_target;
use vbuff_ipc::{ApiScope, ApiTokenIssuer};

fuzz_target!(|data: &[u8]| {
    if let Ok(token) = std::str::from_utf8(data) {
        let issuer = ApiTokenIssuer::from_key([7; 32]);
        let _ = issuer.verify(token, ApiScope::ReadHistory, 1_000);
    }
});
