use std::fmt;

use serde::{Deserialize, Serialize};

use super::{IntegrationContractError, valid_identifier, valid_label};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareDraftState {
    Preview,
    Committed,
    Cancelled,
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShareDraft {
    draft_id: String,
    destination_collection: Option<String>,
    tags: Vec<String>,
    pinned: bool,
    state: ShareDraftState,
}

impl fmt::Debug for ShareDraft {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ShareDraft")
            .field("draft_id", &"[redacted]")
            .field(
                "destination_collection_bytes",
                &self.destination_collection.as_ref().map(String::len),
            )
            .field("tag_count", &self.tags.len())
            .field("pinned", &self.pinned)
            .field("state", &self.state)
            .finish()
    }
}

impl ShareDraft {
    pub fn preview(
        draft_id: String,
        destination_collection: Option<String>,
        tags: Vec<String>,
        pinned: bool,
    ) -> Result<Self, IntegrationContractError> {
        let draft = Self {
            draft_id,
            destination_collection,
            tags,
            pinned,
            state: ShareDraftState::Preview,
        };
        draft.validate()?;
        Ok(draft)
    }

    pub const fn state(&self) -> ShareDraftState {
        self.state
    }

    pub fn draft_id(&self) -> &str {
        &self.draft_id
    }

    pub fn destination_collection(&self) -> Option<&str> {
        self.destination_collection.as_deref()
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub const fn pinned(&self) -> bool {
        self.pinned
    }

    pub fn commit(&mut self) -> Result<(), IntegrationContractError> {
        if self.state != ShareDraftState::Preview {
            return Err(IntegrationContractError::InvalidField);
        }
        self.validate()?;
        self.state = ShareDraftState::Committed;
        Ok(())
    }

    pub fn cancel(&mut self) -> Result<(), IntegrationContractError> {
        if self.state != ShareDraftState::Preview {
            return Err(IntegrationContractError::InvalidField);
        }
        self.state = ShareDraftState::Cancelled;
        Ok(())
    }

    fn validate(&self) -> Result<(), IntegrationContractError> {
        if !valid_identifier(&self.draft_id, 128)
            || self
                .destination_collection
                .as_ref()
                .is_some_and(|collection| !valid_label(collection, 128))
            || self.tags.len() > 32
            || self.tags.iter().any(|tag| !valid_label(tag, 64))
        {
            return Err(IntegrationContractError::InvalidField);
        }
        Ok(())
    }
}
