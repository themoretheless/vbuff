use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::{MAX_SHARED_APPROVERS, all_zero};
use crate::{Result, SyncError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SharedSnippetState {
    Proposed,
    Approved,
    Committed,
    Rejected,
}

/// An output snapshot of locally validated approval state. A future authenticated
/// wire format must verify approver identities before constructing this type.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct SharedSnippetProposal {
    proposal_id: [u8; 16],
    snippet_hash: [u8; 32],
    author_device_hash: [u8; 32],
    base_revision: u64,
    proposed_revision: u64,
    state: SharedSnippetState,
    approvals: BTreeSet<[u8; 32]>,
}

impl fmt::Debug for SharedSnippetProposal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SharedSnippetProposal")
            .field("proposal_id", &"[redacted]")
            .field("snippet_hash", &"[redacted]")
            .field("author_device_hash", &"[redacted]")
            .field("base_revision", &self.base_revision)
            .field("proposed_revision", &self.proposed_revision)
            .field("state", &self.state)
            .field("approval_count", &self.approvals.len())
            .finish()
    }
}

impl SharedSnippetProposal {
    pub fn new(
        proposal_id: [u8; 16],
        snippet_hash: [u8; 32],
        author_device_hash: [u8; 32],
        base_revision: u64,
    ) -> Result<Self> {
        if proposal_id.iter().all(|byte| *byte == 0)
            || all_zero(&snippet_hash)
            || all_zero(&author_device_hash)
        {
            return Err(SyncError::Invalid("invalid snippet proposal id".into()));
        }
        Ok(Self {
            proposal_id,
            snippet_hash,
            author_device_hash,
            base_revision,
            proposed_revision: base_revision
                .checked_add(1)
                .ok_or_else(|| SyncError::Invalid("snippet revision overflow".into()))?,
            state: SharedSnippetState::Proposed,
            approvals: BTreeSet::new(),
        })
    }

    pub const fn proposal_id(&self) -> [u8; 16] {
        self.proposal_id
    }

    pub const fn snippet_hash(&self) -> [u8; 32] {
        self.snippet_hash
    }

    pub const fn author_device_hash(&self) -> [u8; 32] {
        self.author_device_hash
    }

    pub const fn base_revision(&self) -> u64 {
        self.base_revision
    }

    pub const fn proposed_revision(&self) -> u64 {
        self.proposed_revision
    }

    pub const fn state(&self) -> SharedSnippetState {
        self.state
    }

    pub fn approval_count(&self) -> usize {
        self.approvals.len()
    }

    pub fn approve(
        &mut self,
        approver: [u8; 32],
        allowed_approvers: &BTreeSet<[u8; 32]>,
        required: usize,
    ) -> Result<bool> {
        if self.state != SharedSnippetState::Proposed
            || required == 0
            || allowed_approvers.len() > MAX_SHARED_APPROVERS
            || required > allowed_approvers.len()
            || all_zero(&approver)
            || allowed_approvers.iter().any(all_zero)
            || allowed_approvers.contains(&self.author_device_hash)
            || approver == self.author_device_hash
            || !allowed_approvers.contains(&approver)
        {
            return Err(SyncError::Invalid("invalid snippet approval".into()));
        }
        self.approvals.insert(approver);
        if self.approvals.len() >= required {
            self.state = SharedSnippetState::Approved;
        }
        Ok(self.state == SharedSnippetState::Approved)
    }

    pub fn commit(&mut self, current_revision: u64) -> Result<u64> {
        if self.state != SharedSnippetState::Approved || current_revision != self.base_revision {
            return Err(SyncError::Invalid(
                "snippet proposal is not committable".into(),
            ));
        }
        self.state = SharedSnippetState::Committed;
        Ok(self.proposed_revision)
    }
}
