use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use vbuff_types::ContentKind;

#[derive(Clone, PartialEq, Eq)]
pub struct ClipAccessContext<'a> {
    pub kind: ContentKind,
    pub tags: &'a BTreeSet<String>,
    pub collection: Option<&'a str>,
    pub sensitive: bool,
    pub concealed: bool,
    pub sync_eligible: bool,
}

impl fmt::Debug for ClipAccessContext<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClipAccessContext")
            .field("kind", &self.kind)
            .field("tag_count", &self.tags.len())
            .field("has_collection", &self.collection.is_some())
            .field("sensitive", &self.sensitive)
            .field("concealed", &self.concealed)
            .field("sync_eligible", &self.sync_eligible)
            .finish()
    }
}

/// Empty allowlists deny access. Sensitive, concealed, and sync-excluded clips
/// remain non-exportable even if a filter otherwise matches.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClipAccessFilter {
    pub kinds: BTreeSet<ContentKind>,
    pub tags: BTreeSet<String>,
    pub collections: BTreeSet<String>,
}

impl fmt::Debug for ClipAccessFilter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClipAccessFilter")
            .field("kinds", &self.kinds)
            .field("tag_count", &self.tags.len())
            .field("collection_count", &self.collections.len())
            .finish()
    }
}

impl ClipAccessFilter {
    pub fn allows(&self, context: &ClipAccessContext<'_>) -> bool {
        if context.sensitive || context.concealed || !context.sync_eligible {
            return false;
        }
        let kind_allowed = !self.kinds.is_empty() && self.kinds.contains(&context.kind);
        let tag_allowed =
            !self.tags.is_empty() && self.tags.iter().any(|tag| context.tags.contains(tag));
        let collection_allowed = !self.collections.is_empty()
            && context
                .collection
                .is_some_and(|collection| self.collections.contains(collection));
        kind_allowed && (tag_allowed || collection_allowed)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpReadPolicy {
    pub required_tag: String,
    pub read_only: bool,
    pub maximum_results: u16,
}

impl fmt::Debug for McpReadPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("McpReadPolicy")
            .field("required_tag", &"[redacted]")
            .field("read_only", &self.read_only)
            .field("maximum_results", &self.maximum_results)
            .finish()
    }
}

impl Default for McpReadPolicy {
    fn default() -> Self {
        Self {
            required_tag: "AI-shareable".into(),
            read_only: true,
            maximum_results: 20,
        }
    }
}

impl McpReadPolicy {
    pub fn allows(&self, context: &ClipAccessContext<'_>) -> bool {
        self.read_only
            && (1..=100).contains(&self.maximum_results)
            && context.tags.contains(&self.required_tag)
            && !context.sensitive
            && !context.concealed
            && context.sync_eligible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context<'a>(tags: &'a BTreeSet<String>) -> ClipAccessContext<'a> {
        ClipAccessContext {
            kind: ContentKind::Text,
            tags,
            collection: Some("work"),
            sensitive: false,
            concealed: false,
            sync_eligible: true,
        }
    }

    #[test]
    fn empty_filters_deny_and_mcp_requires_explicit_shareable_tag() {
        let tags = BTreeSet::from(["AI-shareable".into()]);
        assert!(!ClipAccessFilter::default().allows(&context(&tags)));
        let filter = ClipAccessFilter {
            kinds: BTreeSet::from([ContentKind::Text]),
            tags: BTreeSet::from(["AI-shareable".into()]),
            collections: BTreeSet::new(),
        };
        assert!(filter.allows(&context(&tags)));
        assert!(McpReadPolicy::default().allows(&context(&tags)));
        let mut sensitive = context(&tags);
        sensitive.sensitive = true;
        assert!(!filter.allows(&sensitive));
        assert!(!McpReadPolicy::default().allows(&sensitive));
        assert!(!format!("{filter:?}").contains("AI-shareable"));
        assert!(!format!("{sensitive:?}").contains("AI-shareable"));
    }
}
