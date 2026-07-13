//! WASM Component Model ABI source shared by host and SDK generators.

pub const ABI_VERSION: u16 = 1;
pub const WIT_SOURCE: &str = include_str!("../wit/vbuff-plugin.wit");

pub fn wit_hash() -> [u8; 32] {
    *blake3::hash(WIT_SOURCE.as_bytes()).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_wit_declares_the_versioned_plugin_world() {
        assert!(WIT_SOURCE.contains("world plugin"));
        assert!(WIT_SOURCE.contains("transform: func"));
        assert_ne!(wit_hash(), [0; 32]);
    }
}
