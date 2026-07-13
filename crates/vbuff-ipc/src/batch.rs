use serde::{Deserialize, Serialize};
use vbuff_types::ClipId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum BatchMutation {
    SetPinned { id: ClipId, pinned: bool },
    SetFavorite { id: ClipId, favorite: bool },
    Delete { id: ClipId },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchRequest {
    pub request_id: String,
    pub mutations: Vec<BatchMutation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchResponse {
    pub request_id: String,
    pub committed: bool,
    pub applied: usize,
    pub error_code: Option<String>,
}

impl BatchRequest {
    pub fn validate(&self, maximum_mutations: usize) -> Result<(), &'static str> {
        if self.request_id.is_empty()
            || self.request_id.len() > 128
            || !self
                .request_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err("request_id_invalid");
        }
        if self.mutations.is_empty() {
            return Err("batch_empty");
        }
        if self.mutations.len() > maximum_mutations {
            return Err("batch_too_large");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_unbounded_batches_are_rejected() {
        let mut request = BatchRequest {
            request_id: "r".into(),
            mutations: Vec::new(),
        };
        assert_eq!(request.validate(100), Err("batch_empty"));
        request.mutations = vec![BatchMutation::Delete { id: ClipId::new() }; 2];
        assert_eq!(request.validate(1), Err("batch_too_large"));
        request.mutations.truncate(1);
        assert_eq!(request.validate(0), Err("batch_too_large"));
    }
}
