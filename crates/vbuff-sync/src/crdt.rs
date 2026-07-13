//! Add-wins observed-remove sets and field-level LWW registers.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::clock::HybridLogicalClock;
use crate::{Result, SyncError};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Dot {
    pub device_id: String,
    pub counter: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "T: Ord + Serialize",
    deserialize = "T: Ord + Deserialize<'de>"
))]
pub struct OrSet<T: Ord> {
    additions: BTreeMap<T, BTreeSet<Dot>>,
    removals: BTreeSet<Dot>,
}

impl<T: Ord> Default for OrSet<T> {
    fn default() -> Self {
        Self {
            additions: BTreeMap::new(),
            removals: BTreeSet::new(),
        }
    }
}

impl<T: Ord + Clone> OrSet<T> {
    pub fn add(&mut self, value: T, dot: Dot) {
        self.additions.entry(value).or_default().insert(dot);
    }

    /// Removes only dots observed locally. Concurrent unseen adds therefore win.
    pub fn remove(&mut self, value: &T) {
        if let Some(dots) = self.additions.get(value) {
            self.removals.extend(dots.iter().cloned());
        }
    }

    pub fn contains(&self, value: &T) -> bool {
        self.additions
            .get(value)
            .is_some_and(|dots| dots.iter().any(|dot| !self.removals.contains(dot)))
    }

    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.additions.keys().filter(|value| self.contains(value))
    }

    pub fn merge(&mut self, other: &Self) {
        for (value, dots) in &other.additions {
            self.additions
                .entry(value.clone())
                .or_default()
                .extend(dots.iter().cloned());
        }
        self.removals.extend(other.removals.iter().cloned());
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LwwRegister<T> {
    pub value: T,
    pub clock: HybridLogicalClock,
}

impl<T: Clone> LwwRegister<T> {
    pub fn assign(&mut self, value: T, clock: HybridLogicalClock, now_ms: u64) {
        let clock = clock.bounded_at(now_ms);
        if clock > self.clock {
            self.value = value;
            self.clock = clock;
        }
    }

    pub fn merge(&mut self, other: &Self, now_ms: u64) {
        self.assign(other.value.clone(), other.clock.clone(), now_ms);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemFields {
    pub name: LwwRegister<Option<String>>,
    pub notes: LwwRegister<Option<String>>,
    pub color: LwwRegister<Option<String>>,
    pub pinned: LwwRegister<bool>,
    pub tags: OrSet<String>,
}

impl ItemFields {
    pub fn merge(&mut self, other: &Self, now_ms: u64) {
        self.name.merge(&other.name, now_ms);
        self.notes.merge(&other.notes, now_ms);
        self.color.merge(&other.color, now_ms);
        self.pinned.merge(&other.pinned, now_ms);
        self.tags.merge(&other.tags);
    }
}

/// Item presence is an add-wins OR-Set. Pinning emits a fresh presence dot, so
/// an unobserved concurrent delete cannot erase the pinned item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemReplica {
    pub item_id: String,
    pub presence: OrSet<String>,
    pub fields: ItemFields,
}

impl ItemReplica {
    pub fn new(item_id: impl Into<String>, fields: ItemFields, dot: Dot) -> Self {
        let item_id = item_id.into();
        let mut presence = OrSet::default();
        presence.add(item_id.clone(), dot);
        Self {
            item_id,
            presence,
            fields,
        }
    }

    pub fn delete(&mut self) {
        self.presence.remove(&self.item_id);
    }

    pub fn pin(&mut self, dot: Dot, clock: HybridLogicalClock, now_ms: u64) {
        self.presence.add(self.item_id.clone(), dot);
        self.fields.pinned.assign(true, clock, now_ms);
    }

    pub fn is_present(&self) -> bool {
        self.presence.contains(&self.item_id)
    }

    pub fn merge(&mut self, other: &Self, now_ms: u64) -> Result<()> {
        if self.item_id != other.item_id {
            return Err(SyncError::Invalid(
                "cannot merge replicas with different item IDs".into(),
            ));
        }
        self.presence.merge(&other.presence);
        self.fields.merge(&other.fields, now_ms);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dot(device: &str, counter: u64) -> Dot {
        Dot {
            device_id: device.into(),
            counter,
        }
    }

    #[test]
    fn concurrent_add_wins_over_unobserved_remove() {
        let mut left = OrSet::default();
        left.add("pinned", dot("a", 1));
        let mut right = left.clone();
        left.remove(&"pinned");
        right.add("pinned", dot("b", 1));

        left.merge(&right);
        right.merge(&left);
        assert!(left.contains(&"pinned"));
        assert_eq!(left, right);
    }

    #[test]
    fn independent_fields_survive_concurrent_edits() {
        let base = HybridLogicalClock::new("a", 1);
        let mut left = ItemFields {
            name: LwwRegister {
                value: None,
                clock: base.clone(),
            },
            notes: LwwRegister {
                value: None,
                clock: base.clone(),
            },
            color: LwwRegister {
                value: None,
                clock: base.clone(),
            },
            pinned: LwwRegister {
                value: false,
                clock: base.clone(),
            },
            tags: OrSet::default(),
        };
        let mut right = left.clone();
        left.notes
            .assign(Some("note".into()), HybridLogicalClock::new("a", 2), 2);
        right
            .color
            .assign(Some("red".into()), HybridLogicalClock::new("b", 2), 2);
        left.merge(&right, 2);
        assert_eq!(left.notes.value.as_deref(), Some("note"));
        assert_eq!(left.color.value.as_deref(), Some("red"));
    }

    #[test]
    fn concurrent_delete_and_pin_keeps_the_item() {
        let base = HybridLogicalClock::new("a", 1);
        let fields = ItemFields {
            name: LwwRegister {
                value: None,
                clock: base.clone(),
            },
            notes: LwwRegister {
                value: None,
                clock: base.clone(),
            },
            color: LwwRegister {
                value: None,
                clock: base.clone(),
            },
            pinned: LwwRegister {
                value: false,
                clock: base,
            },
            tags: OrSet::default(),
        };
        let mut deleted = ItemReplica::new("clip-1", fields, dot("a", 1));
        let mut pinned = deleted.clone();
        deleted.delete();
        pinned.pin(dot("b", 1), HybridLogicalClock::new("b", 2), 2);

        deleted.merge(&pinned, 2).unwrap();
        pinned.merge(&deleted, 2).unwrap();
        assert!(deleted.is_present());
        assert!(deleted.fields.pinned.value);
        assert_eq!(deleted, pinned);
    }

    #[test]
    fn far_future_register_cannot_win_forever() {
        let mut register = LwwRegister {
            value: "local",
            clock: HybridLogicalClock::new("a", 1_000),
        };
        register.assign(
            "hostile",
            HybridLogicalClock {
                physical_ms: u64::MAX,
                logical: 0,
                node_id: "b".into(),
            },
            1_000,
        );
        register.assign(
            "recovered",
            HybridLogicalClock::new("a", 1_000 + crate::clock::MAX_REMOTE_FUTURE_MS + 1),
            1_000 + crate::clock::MAX_REMOTE_FUTURE_MS + 1,
        );
        assert_eq!(register.value, "recovered");
    }

    #[test]
    fn replicas_with_different_ids_never_merge() {
        let clock = HybridLogicalClock::new("a", 1);
        let fields = ItemFields {
            name: LwwRegister {
                value: None,
                clock: clock.clone(),
            },
            notes: LwwRegister {
                value: None,
                clock: clock.clone(),
            },
            color: LwwRegister {
                value: None,
                clock: clock.clone(),
            },
            pinned: LwwRegister {
                value: false,
                clock,
            },
            tags: OrSet::default(),
        };
        let mut left = ItemReplica::new("clip-a", fields.clone(), dot("a", 1));
        let right = ItemReplica::new("clip-b", fields, dot("b", 1));

        assert!(left.merge(&right, 1).is_err());
        assert_eq!(left.item_id, "clip-a");
    }
}
