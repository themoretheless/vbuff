#![no_main]

use libfuzzer_sys::fuzz_target;
use vbuff_plugin::PluginManifest;

fuzz_target!(|data: &[u8]| {
    if let Ok(manifest) = serde_json::from_slice::<PluginManifest>(data) {
        let _ = manifest.validate();
        let _ = manifest.canonical_bytes();
    }
});
