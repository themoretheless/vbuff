use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use vbuff_types::ClipId;

use crate::Capability;

const MAX_FILTER_COLLECTIONS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Captured,
    Updated,
    Deleted,
    Pasted,
    HealthChanged,
    SecurityChanged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub sequence: u64,
    pub kind: EventKind,
    pub clip_id: Option<ClipId>,
    pub collection_id: Option<String>,
    pub sensitive: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EventFilter {
    pub kinds: BTreeSet<EventKind>,
    pub collection_ids: BTreeSet<String>,
    pub include_sensitive: bool,
}

impl EventFilter {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.collection_ids.len() > MAX_FILTER_COLLECTIONS {
            return Err("too_many_collection_filters");
        }
        if self.collection_ids.iter().any(|collection| {
            collection.is_empty()
                || collection.len() > 128
                || !collection
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        }) {
            return Err("invalid_collection_filter");
        }
        Ok(())
    }

    /// Match ordinary events. Sensitive events always fail closed here.
    pub fn matches(&self, event: &EventEnvelope) -> bool {
        self.matches_metadata(event) && !event.sensitive
    }

    /// Match after the server has independently granted sensitive-event access.
    pub fn matches_with_grants(
        &self,
        event: &EventEnvelope,
        granted: &BTreeSet<Capability>,
    ) -> bool {
        self.matches_metadata(event)
            && (!event.sensitive
                || (self.include_sensitive && granted.contains(&Capability::ReadSensitiveHistory)))
    }

    fn matches_metadata(&self, event: &EventEnvelope) -> bool {
        self.validate().is_ok()
            && (self.kinds.is_empty() || self.kinds.contains(&event.kind))
            && (self.collection_ids.is_empty()
                || event
                    .collection_id
                    .as_ref()
                    .is_some_and(|collection| self.collection_ids.contains(collection)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_fail_closed_for_sensitive_events() {
        let event = EventEnvelope {
            sequence: 1,
            kind: EventKind::Captured,
            clip_id: Some(ClipId::new()),
            collection_id: Some("work".into()),
            sensitive: true,
        };
        assert!(!EventFilter::default().matches(&event));
        let requested = EventFilter {
            include_sensitive: true,
            ..EventFilter::default()
        };
        assert!(!requested.matches(&event));
        assert!(!requested.matches_with_grants(&event, &BTreeSet::new()));
        assert!(
            requested
                .matches_with_grants(&event, &BTreeSet::from([Capability::ReadSensitiveHistory]))
        );

        let invalid = EventFilter {
            collection_ids: BTreeSet::from(["bad/collection".into()]),
            ..EventFilter::default()
        };
        assert_eq!(invalid.validate(), Err("invalid_collection_filter"));
        assert!(
            !invalid
                .matches_with_grants(&event, &BTreeSet::from([Capability::ReadSensitiveHistory]))
        );
    }
}
