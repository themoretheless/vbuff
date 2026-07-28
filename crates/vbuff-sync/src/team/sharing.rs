use serde::{Deserialize, Serialize};

use super::{TeamRole, invalid, validate_hash, validate_id};
use crate::Result;

const MAX_COMMENT_BYTES: usize = 2 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedClipLease {
    pub item_id: String,
    pub expires_at_ms: i64,
}

impl SharedClipLease {
    pub fn validate(&self) -> Result<()> {
        validate_id(&self.item_id)?;
        if self.expires_at_ms <= 0 {
            return invalid("shared clip expiry must be positive");
        }
        Ok(())
    }

    pub fn is_active(&self, now_ms: i64) -> bool {
        now_ms >= 0 && self.validate().is_ok() && now_ms < self.expires_at_ms
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalShareGrant {
    pub share_id: String,
    pub item_hash: [u8; 32],
    pub expires_at_ms: i64,
    pub revoked_at_ms: Option<i64>,
}

impl ExternalShareGrant {
    pub fn validate(&self) -> Result<()> {
        validate_id(&self.share_id)?;
        validate_hash(&self.item_hash, "external share item hash is invalid")?;
        if self.expires_at_ms <= 0 {
            return invalid("share expiry must be positive");
        }
        if self
            .revoked_at_ms
            .is_some_and(|revoked| revoked < 0 || revoked > self.expires_at_ms)
        {
            return invalid("share revocation cannot follow expiry");
        }
        Ok(())
    }

    pub fn is_active(&self, now_ms: i64) -> bool {
        now_ms >= 0
            && self.validate().is_ok()
            && now_ms < self.expires_at_ms
            && self.revoked_at_ms.is_none_or(|revoked| now_ms < revoked)
    }

    pub fn revoke(&mut self, now_ms: i64) -> Result<()> {
        self.validate()?;
        if now_ms < 0 {
            return invalid("share revocation timestamp is invalid");
        }
        if self.revoked_at_ms.is_none() {
            self.revoked_at_ms = Some(now_ms.min(self.expires_at_ms));
        }
        Ok(())
    }
}

impl std::fmt::Debug for ExternalShareGrant {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExternalShareGrant")
            .field("share_id", &self.share_id)
            .field("item_hash", &"[redacted]")
            .field("expires_at_ms", &self.expires_at_ms)
            .field("revoked_at_ms", &self.revoked_at_ms)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionForkPlan {
    pub source_collection_id: String,
    pub private_collection_id: String,
    pub source_revision: u64,
    pub item_count: usize,
}

impl CollectionForkPlan {
    pub fn validate(&self, maximum_items: usize) -> Result<()> {
        validate_id(&self.source_collection_id)?;
        validate_id(&self.private_collection_id)?;
        if self.source_collection_id == self.private_collection_id {
            return invalid("fork target must differ from source");
        }
        if self.item_count > maximum_items {
            return invalid("fork exceeds item limit");
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictComment {
    pub comment_id: String,
    pub snippet_id: String,
    pub author_hash: [u8; 32],
    pub body: String,
    pub created_at_ms: i64,
}

impl ConflictComment {
    pub fn validate(&self, role: TeamRole) -> Result<()> {
        if !role.can_comment() {
            return invalid("role cannot comment");
        }
        validate_id(&self.comment_id)?;
        validate_id(&self.snippet_id)?;
        validate_hash(&self.author_hash, "comment author hash is invalid")?;
        if self.body.trim().is_empty()
            || self.body.len() > MAX_COMMENT_BYTES
            || self.body.chars().any(char::is_control)
            || self.created_at_ms < 0
        {
            return invalid("invalid conflict comment");
        }
        Ok(())
    }
}

impl std::fmt::Debug for ConflictComment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConflictComment")
            .field("comment_id", &self.comment_id)
            .field("snippet_id", &self.snippet_id)
            .field("author_hash", &"[redacted]")
            .field("body", &"[redacted]")
            .field("created_at_ms", &self.created_at_ms)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BroadcastPriority {
    Important,
    Emergency,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmergencyBroadcast {
    pub broadcast_id: String,
    pub revision: u64,
    pub priority: BroadcastPriority,
    pub message: String,
    pub expires_at_ms: i64,
}

impl EmergencyBroadcast {
    pub fn validate(&self, role: TeamRole, now_ms: i64) -> Result<()> {
        if !role.can_manage_policy() {
            return invalid("only an owner can publish an emergency broadcast");
        }
        validate_id(&self.broadcast_id)?;
        if now_ms < 0
            || self.revision == 0
            || self.message.trim().is_empty()
            || self.message.len() > MAX_COMMENT_BYTES
            || self.message.chars().any(char::is_control)
            || self.expires_at_ms <= now_ms
        {
            return invalid("invalid emergency broadcast");
        }
        Ok(())
    }
}

impl std::fmt::Debug for EmergencyBroadcast {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EmergencyBroadcast")
            .field("broadcast_id", &self.broadcast_id)
            .field("revision", &self.revision)
            .field("priority", &self.priority)
            .field("message", &"[redacted]")
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}
