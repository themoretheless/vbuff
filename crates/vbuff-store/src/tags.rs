pub(crate) const TAG_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS tags (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, color TEXT NOT NULL); CREATE TABLE IF NOT EXISTS clip_tags (clip_id TEXT NOT NULL, tag_id TEXT NOT NULL, PRIMARY KEY(clip_id, tag_id)); DELETE FROM clip_tags WHERE clip_id NOT IN (SELECT id FROM clips);";
// Shared tag operations; identical parameterized SQL on both engines.
use crate::{Result, Store, StoreError, params};
use std::collections::HashSet;
use vbuff_types::{ClipId, TagCommand, TagRecord, TagSnapshot};

impl Store {
    pub(crate) fn init_tags(&self) -> Result<()> {
        self.conn.execute_batch(TAG_SCHEMA)?;
        Ok(())
    }
    pub fn tag_snapshot(&self) -> Result<TagSnapshot> {
        let mut statement = self
            .conn
            .prepare("SELECT id, name, color FROM tags ORDER BY name")?;
        let rows = statement.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut tags = Vec::new();
        for row in rows {
            let (id, name, color) = row?;
            tags.push(TagRecord {
                id,
                name,
                color: serde_json::from_str(&color)?,
                clips: HashSet::new(),
            });
        }
        let mut statement = self.conn.prepare("SELECT ct.clip_id, ct.tag_id FROM clip_tags ct JOIN clips c ON c.id = ct.clip_id WHERE c.expires_at IS NULL OR c.expires_at > $1")?;
        let rows = statement.query_map(params![chrono::Utc::now().timestamp_millis()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (clip, tag) = row?;
            if let Some(record) = tags.iter_mut().find(|t| t.id == tag) {
                let id = ClipId::parse(&clip)
                    .map_err(|_| StoreError::Corrupt("invalid tag clip id".into()))?;
                record.clips.insert(id);
            }
        }
        Ok(TagSnapshot { tags })
    }
    pub fn edit_tags(&self, command: &TagCommand) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let invalid = || StoreError::Maintenance("invalid tag operation".into());
        let exists = |id: &str| -> Result<bool> {
            Ok(tx.query_row(
                "SELECT COUNT(*) FROM tags WHERE id = $1",
                params![id],
                |r| r.get::<_, i64>(0),
            )? == 1)
        };
        match command {
            TagCommand::Save { id, name, color } => {
                let name = name.trim().to_lowercase();
                if name.is_empty() || name.len() > 64 || name.chars().any(char::is_control) {
                    return Err(invalid());
                }
                let color = serde_json::to_string(color)?;
                if let Some(id) = id {
                    if !exists(id)? {
                        return Err(invalid());
                    }
                    tx.execute(
                        "UPDATE tags SET name = $1, color = $2 WHERE id = $3",
                        params![name, color, id],
                    )?;
                } else {
                    let count: i64 = tx.query_row("SELECT COUNT(*) FROM tags", [], |r| r.get(0))?;
                    if count >= 256 {
                        return Err(invalid());
                    }
                    tx.execute(
                        "INSERT INTO tags VALUES ($1, $2, $3)",
                        params![ClipId::new().to_string_repr(), name, color],
                    )?;
                }
            }
            TagCommand::Delete(id) => {
                if !exists(id)? {
                    return Err(invalid());
                }
                tx.execute("DELETE FROM clip_tags WHERE tag_id = $1", params![id])?;
                tx.execute("DELETE FROM tags WHERE id = $1", params![id])?;
            }
            TagCommand::Merge { source, target } => {
                if source == target || !exists(source)? || !exists(target)? {
                    return Err(invalid());
                }
                tx.execute("INSERT INTO clip_tags SELECT clip_id, $1 FROM clip_tags WHERE tag_id = $2 ON CONFLICT DO NOTHING", params![target, source])?;
                tx.execute("DELETE FROM clip_tags WHERE tag_id = $1", params![source])?;
                tx.execute("DELETE FROM tags WHERE id = $1", params![source])?;
            }
            TagCommand::Assign {
                clips,
                tag,
                assigned,
            } => {
                if clips.is_empty() || clips.len() > 1000 || !exists(tag)? {
                    return Err(invalid());
                }
                for id in clips {
                    let metadata: String = tx.query_row(
                        "SELECT metadata_json FROM clips WHERE id = $1 AND (expires_at IS NULL OR expires_at > $2)",
                        params![id.to_string_repr(), chrono::Utc::now().timestamp_millis()], |r| r.get(0))?;
                    if serde_json::from_str::<crate::StoredMetadata>(&metadata)?.sensitive {
                        return Err(invalid());
                    }
                    if *assigned {
                        let count: i64 = tx.query_row(
                            "SELECT COUNT(*) FROM clip_tags WHERE clip_id = $1",
                            params![id.to_string_repr()],
                            |r| r.get(0),
                        )?;
                        let present: i64 = tx.query_row(
                            "SELECT COUNT(*) FROM clip_tags WHERE clip_id = $1 AND tag_id = $2",
                            params![id.to_string_repr(), tag],
                            |r| r.get(0),
                        )?;
                        if count >= 32 && present == 0 {
                            return Err(invalid());
                        }
                        tx.execute(
                            "INSERT INTO clip_tags VALUES ($1, $2) ON CONFLICT DO NOTHING",
                            params![id.to_string_repr(), tag],
                        )?;
                    } else {
                        tx.execute(
                            "DELETE FROM clip_tags WHERE clip_id = $1 AND tag_id = $2",
                            params![id.to_string_repr(), tag],
                        )?;
                    }
                }
            }
        }
        tx.execute(
            "DELETE FROM clip_tags WHERE clip_id NOT IN (SELECT id FROM clips)",
            [],
        )?;
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM clip_tags", [], |r| r.get(0))?;
        if count > 65536 {
            return Err(invalid());
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_types::{Clip, ClipMeta, ContentKind, Flavor};
    fn clip(text: &str) -> Clip {
        let flavors = vec![Flavor::inline("text/plain", text.as_bytes().to_vec())];
        Clip {
            id: ClipId::new(),
            content_hash: vbuff_core::content_hash_from_flavors(&flavors),
            meta: ClipMeta::now(ContentKind::Text, text.len() as u64, None),
            flavors,
            pinned: false,
            favorite: false,
        }
    }
    fn create(store: &Store, name: &str) -> String {
        store
            .edit_tags(&TagCommand::Save {
                id: None,
                name: name.into(),
                color: [12, 34, 56],
            })
            .unwrap();
        store
            .tag_snapshot()
            .unwrap()
            .tags
            .into_iter()
            .find(|t| t.name == name)
            .unwrap()
            .id
    }
    #[test]
    fn tags_survive_reopen_merge_overlap_and_delete_without_deleting_clips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(if crate::BACKEND == "duckdb" {
            "history.duckdb"
        } else {
            "history.db"
        });
        let a = clip("alpha");
        let b = clip("beta");
        let target;
        {
            let store = Store::open(&path).unwrap();
            store.insert(&a).unwrap();
            store.insert(&b).unwrap();
            let source = create(&store, "source");
            target = create(&store, "target");
            store
                .edit_tags(&TagCommand::Assign {
                    clips: vec![a.id, b.id],
                    tag: source.clone(),
                    assigned: true,
                })
                .unwrap();
            store
                .edit_tags(&TagCommand::Assign {
                    clips: vec![b.id],
                    tag: target.clone(),
                    assigned: true,
                })
                .unwrap();
            store
                .edit_tags(&TagCommand::Merge {
                    source,
                    target: target.clone(),
                })
                .unwrap();
            store
                .edit_tags(&TagCommand::Save {
                    id: Some(target.clone()),
                    name: "Renamed".into(),
                    color: [1, 2, 3],
                })
                .unwrap();
        }
        let store = Store::open(&path).unwrap();
        let snapshot = store.tag_snapshot().unwrap();
        assert_eq!(snapshot.tags.len(), 1);
        assert_eq!(snapshot.tags[0].name, "renamed");
        assert_eq!(snapshot.tags[0].color, [1, 2, 3]);
        assert_eq!(snapshot.tags[0].clips.len(), 2);
        store.delete(a.id).unwrap();
        assert_eq!(store.tag_snapshot().unwrap().tags[0].clips.len(), 1);
        let dangling: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM clip_tags WHERE clip_id = $1",
                params![a.id.to_string_repr()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(dangling, 0);
        store.edit_tags(&TagCommand::Delete(target)).unwrap();
        assert!(store.tag_snapshot().unwrap().tags.is_empty());
        assert!(store.get_clip(b.id).unwrap().is_some());
    }
    #[test]
    fn tag_assignment_is_atomic_and_names_are_unique() {
        let store = Store::open_in_memory().unwrap();
        let clip = clip("valid");
        store.insert(&clip).unwrap();
        let id = create(&store, "work");
        assert!(
            store
                .edit_tags(&TagCommand::Save {
                    id: None,
                    name: " WORK ".into(),
                    color: [0; 3]
                })
                .is_err()
        );
        assert!(
            store
                .edit_tags(&TagCommand::Assign {
                    clips: vec![clip.id, ClipId::new()],
                    tag: id.clone(),
                    assigned: true
                })
                .is_err()
        );
        assert!(store.tag_snapshot().unwrap().tags[0].clips.is_empty());
        assert!(
            store
                .edit_tags(&TagCommand::Merge {
                    source: id,
                    target: "missing".into()
                })
                .is_err()
        );
        assert_eq!(store.tag_snapshot().unwrap().tags.len(), 1);
    }
    #[test]
    fn tags_all_any_and_unassignment() {
        let store = Store::open_in_memory().unwrap();
        let clip = clip("filters");
        store.insert(&clip).unwrap();
        let a = create(&store, "a");
        let b = create(&store, "b");
        store
            .edit_tags(&TagCommand::Assign {
                clips: vec![clip.id],
                tag: a.clone(),
                assigned: true,
            })
            .unwrap();
        let snapshot = store.tag_snapshot().unwrap();
        assert!(snapshot.matches(clip.id, &[a.clone(), b.clone()], false));
        assert!(!snapshot.matches(clip.id, &[a.clone(), b], true));
        store
            .edit_tags(&TagCommand::Assign {
                clips: vec![clip.id],
                tag: a,
                assigned: false,
            })
            .unwrap();
        assert!(
            store
                .tag_snapshot()
                .unwrap()
                .tags
                .iter()
                .all(|t| t.clips.is_empty())
        );
    }
}
