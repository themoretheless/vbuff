use crate::ClipId;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagRecord {
    pub id: String,
    pub name: String,
    pub color: [u8; 3],
    pub clips: HashSet<ClipId>,
}
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagSnapshot {
    pub tags: Vec<TagRecord>,
}
impl TagSnapshot {
    pub fn matches(&self, clip: ClipId, ids: &[String], all: bool) -> bool {
        let has = |id: &String| {
            self.tags
                .iter()
                .any(|t| &t.id == id && t.clips.contains(&clip))
        };
        ids.is_empty()
            || if all {
                ids.iter().all(has)
            } else {
                ids.iter().any(has)
            }
    }
}
#[derive(Clone, PartialEq, Eq)]
pub enum TagCommand {
    Save {
        id: Option<String>,
        name: String,
        color: [u8; 3],
    },
    Delete(String),
    Merge {
        source: String,
        target: String,
    },
    Assign {
        clips: Vec<ClipId>,
        tag: String,
        assigned: bool,
    },
}
impl std::fmt::Debug for TagCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TagCommand([redacted])")
    }
}
