use serde::{Deserialize, Serialize};

use super::{invalid, validate_hash, validate_id};
use crate::{Result, SyncError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamRole {
    Owner,
    Editor,
    Commenter,
    Viewer,
}

impl TeamRole {
    pub const fn can_edit(self) -> bool {
        matches!(self, Self::Owner | Self::Editor)
    }

    pub const fn can_comment(self) -> bool {
        matches!(self, Self::Owner | Self::Editor | Self::Commenter)
    }

    pub const fn can_manage_policy(self) -> bool {
        matches!(self, Self::Owner)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnippetPublicationState {
    Draft,
    Approved,
    Published,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnippetApprovalWorkflow {
    pub snippet_id: String,
    pub revision: u64,
    pub author_hash: [u8; 32],
    pub reviewer_hash: Option<[u8; 32]>,
    pub state: SnippetPublicationState,
}

impl std::fmt::Debug for SnippetApprovalWorkflow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SnippetApprovalWorkflow")
            .field("snippet_id", &self.snippet_id)
            .field("revision", &self.revision)
            .field("author_hash", &"[redacted]")
            .field("reviewer_hash", &self.reviewer_hash.map(|_| "[redacted]"))
            .field("state", &self.state)
            .finish()
    }
}

impl SnippetApprovalWorkflow {
    pub fn new(snippet_id: impl Into<String>, author_hash: [u8; 32]) -> Result<Self> {
        let snippet_id = snippet_id.into();
        validate_id(&snippet_id)?;
        validate_hash(&author_hash, "snippet author hash is invalid")?;
        Ok(Self {
            snippet_id,
            revision: 1,
            author_hash,
            reviewer_hash: None,
            state: SnippetPublicationState::Draft,
        })
    }

    pub fn approve(&mut self, role: TeamRole, reviewer_hash: [u8; 32]) -> Result<()> {
        if !role.can_edit() {
            return invalid("role cannot approve snippets");
        }
        validate_hash(&reviewer_hash, "snippet reviewer hash is invalid")?;
        if reviewer_hash == self.author_hash {
            return invalid("snippet approval requires a distinct reviewer");
        }
        if self.state != SnippetPublicationState::Draft {
            return invalid("only a draft can be approved");
        }
        self.reviewer_hash = Some(reviewer_hash);
        self.state = SnippetPublicationState::Approved;
        Ok(())
    }

    pub fn publish(&mut self, role: TeamRole) -> Result<()> {
        if !role.can_edit() || self.state != SnippetPublicationState::Approved {
            return invalid("publication requires an approved revision and editor role");
        }
        self.state = SnippetPublicationState::Published;
        Ok(())
    }

    pub fn revise(&mut self, role: TeamRole, author_hash: [u8; 32]) -> Result<()> {
        if !role.can_edit() {
            return invalid("role cannot revise snippets");
        }
        validate_hash(&author_hash, "snippet author hash is invalid")?;
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| SyncError::Invalid("snippet revision overflow".into()))?;
        self.author_hash = author_hash;
        self.reviewer_hash = None;
        self.state = SnippetPublicationState::Draft;
        Ok(())
    }
}
