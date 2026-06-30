//! JSON (de)serialization helpers for the flavor blob column.
//!
//! Flavors carry binary payloads; `serde_json` would render `Vec<u8>` as a JSON
//! array of numbers, which is correct but bulky. For the MVP that is acceptable
//! and keeps the schema trivially inspectable; a later revision can switch to a
//! compact binary encoding without changing the public API.

use vbuff_types::Flavor;

use crate::Result;

/// Serialize a flavor set to a JSON string.
pub fn flavors_to_json(flavors: &[Flavor]) -> Result<String> {
    Ok(serde_json::to_string(flavors)?)
}

/// Deserialize a flavor set from a JSON string.
pub fn flavors_from_json(json: &str) -> Result<Vec<Flavor>> {
    Ok(serde_json::from_str(json)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_types::Flavor;

    #[test]
    fn roundtrip() {
        let flavors = vec![
            Flavor::inline("text/plain", b"hi".to_vec()),
            Flavor::inline("image/png", vec![0x89, 0x50, 0x4e, 0x47]),
        ];
        let json = flavors_to_json(&flavors).unwrap();
        let back = flavors_from_json(&json).unwrap();
        assert_eq!(flavors, back);
    }
}
