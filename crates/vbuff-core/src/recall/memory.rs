use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt;

use vbuff_types::{Clip, ClipId};

use crate::secret::detect_secrets;

const MAX_MACROS: usize = 64;
const MAX_MACRO_NAME_BYTES: usize = 32;
const MAX_QUERY_BYTES: usize = 4 * 1_024;
const MAX_ALIAS_BYTES: usize = 64;
const MAX_ALIASES: usize = 2_048;
const MAX_ALIASES_PER_CLIP: usize = 8;
const MAX_AFFINITIES: usize = 8_192;
const MAX_HISTORY: usize = 100;
const MAX_SCOPE_BYTES: usize = 512;
const MAX_TAGS: usize = 8_192;
const MAX_TAGS_PER_CLIP: usize = 64;
const MAX_QUERY_PIN_GROUPS: usize = 128;
const MAX_QUERY_PINS: usize = 64;

#[derive(Clone, Default)]
pub struct SearchMacroRegistry {
    macros: BTreeMap<String, String>,
}

impl fmt::Debug for SearchMacroRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SearchMacroRegistry")
            .field("macro_count", &self.macros.len())
            .finish()
    }
}

impl SearchMacroRegistry {
    pub fn set(&mut self, name: &str, query: &str) -> bool {
        let name = normalize_label(name);
        if !valid_macro_name(&name)
            || query.trim().is_empty()
            || query.len() > MAX_QUERY_BYTES
            || query.chars().any(char::is_control)
            || (!self.macros.contains_key(&name) && self.macros.len() >= MAX_MACROS)
        {
            return false;
        }
        self.macros.insert(name, query.trim().to_owned());
        true
    }

    pub fn expand(&self, input: &str) -> Option<String> {
        if input.len() > MAX_QUERY_BYTES {
            return None;
        }
        let mut expanded = input.trim().to_owned();
        let mut seen = BTreeSet::new();
        for _ in 0..8 {
            let first = expanded.split_whitespace().next()?;
            let Some(name) = first.strip_prefix('@') else {
                return Some(expanded);
            };
            let name = normalize_label(name);
            if !seen.insert(name.clone()) {
                return None;
            }
            let replacement = self.macros.get(&name)?;
            let suffix = expanded[first.len()..].trim();
            expanded = if suffix.is_empty() {
                replacement.clone()
            } else {
                format!("{replacement} {suffix}")
            };
            if expanded.len() > MAX_QUERY_BYTES {
                return None;
            }
        }
        None
    }
}

#[derive(Clone, Default)]
pub struct PinnedAliases {
    by_alias: BTreeMap<String, ClipId>,
    per_clip: HashMap<ClipId, usize>,
}

impl fmt::Debug for PinnedAliases {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PinnedAliases")
            .field("alias_count", &self.by_alias.len())
            .field("clip_count", &self.per_clip.len())
            .finish()
    }
}

impl PinnedAliases {
    pub fn add(&mut self, clip_id: ClipId, pinned: bool, alias: &str) -> bool {
        let alias = normalize_label(alias);
        if !pinned
            || !valid_label(&alias, MAX_ALIAS_BYTES)
            || self.by_alias.contains_key(&alias)
            || self.by_alias.len() >= MAX_ALIASES
            || self.per_clip.get(&clip_id).copied().unwrap_or_default() >= MAX_ALIASES_PER_CLIP
        {
            return false;
        }
        self.by_alias.insert(alias, clip_id);
        *self.per_clip.entry(clip_id).or_default() += 1;
        true
    }

    pub fn remove(&mut self, alias: &str) -> bool {
        let alias = normalize_label(alias);
        let Some(clip_id) = self.by_alias.remove(&alias) else {
            return false;
        };
        if let Some(count) = self.per_clip.get_mut(&clip_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.per_clip.remove(&clip_id);
            }
        }
        true
    }

    pub fn match_score(&self, clip_id: ClipId, query: &str) -> Option<i64> {
        let query = normalize_label(query);
        if query.is_empty() {
            return None;
        }
        self.by_alias.iter().find_map(|(alias, candidate)| {
            (*candidate == clip_id && alias.starts_with(&query)).then_some(if alias == &query {
                220
            } else {
                180
            })
        })
    }
}

#[derive(Clone, Default)]
pub struct PasteAffinity {
    counts: HashMap<([u8; 32], [u8; 32]), u16>,
}

impl fmt::Debug for PasteAffinity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PasteAffinity")
            .field("pair_count", &self.counts.len())
            .finish()
    }
}

impl PasteAffinity {
    pub fn record(&mut self, destination_app: &str, content_hash: [u8; 32]) -> bool {
        let Some(app_hash) = identity_hash(destination_app, MAX_SCOPE_BYTES) else {
            return false;
        };
        let key = (app_hash, content_hash);
        if !self.counts.contains_key(&key) && self.counts.len() >= MAX_AFFINITIES {
            return false;
        }
        let count = self.counts.entry(key).or_default();
        *count = count.saturating_add(1);
        true
    }

    pub fn boost(&self, destination_app: &str, content_hash: [u8; 32]) -> i64 {
        let Some(app_hash) = identity_hash(destination_app, MAX_SCOPE_BYTES) else {
            return 0;
        };
        i64::from(
            self.counts
                .get(&(app_hash, content_hash))
                .copied()
                .unwrap_or_default()
                .min(20),
        ) * 4
    }
}

#[derive(Clone)]
pub struct QueryHistory {
    entries: VecDeque<String>,
    capacity: usize,
}

impl fmt::Debug for QueryHistory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("QueryHistory")
            .field("entry_count", &self.entries.len())
            .field("capacity", &self.capacity)
            .finish()
    }
}

impl QueryHistory {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity.min(MAX_HISTORY)),
            capacity: capacity.min(MAX_HISTORY),
        }
    }

    pub fn remember(&mut self, query: &str) -> bool {
        let query = query.trim();
        if self.capacity == 0
            || query.is_empty()
            || query.len() > MAX_QUERY_BYTES
            || query.chars().any(char::is_control)
            || query_looks_sensitive(query)
        {
            return false;
        }
        self.entries.retain(|entry| entry != query);
        if self.entries.len() == self.capacity {
            self.entries.pop_back();
        }
        self.entries.push_front(query.to_owned());
        true
    }

    pub fn entries(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(String::as_str)
    }
}

impl Default for QueryHistory {
    fn default() -> Self {
        Self::new(20)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchScope {
    All,
    App(String),
    Device(String),
    Collection(String),
}

#[derive(Clone, Debug, Default)]
pub struct SearchScopeLock {
    scope: Option<SearchScope>,
}

impl SearchScopeLock {
    pub fn set(&mut self, scope: SearchScope) -> bool {
        let valid = match &scope {
            SearchScope::All => true,
            SearchScope::App(value)
            | SearchScope::Device(value)
            | SearchScope::Collection(value) => valid_label(value, MAX_SCOPE_BYTES),
        };
        if valid {
            self.scope = (!matches!(scope, SearchScope::All)).then_some(scope);
        }
        valid
    }

    pub fn clear(&mut self) {
        self.scope = None;
    }

    pub fn scope(&self) -> Option<&SearchScope> {
        self.scope.as_ref()
    }

    pub fn matches(&self, clip: &Clip, tags: Option<&ClipTags>) -> bool {
        match self.scope.as_ref() {
            None | Some(SearchScope::All) => true,
            Some(SearchScope::App(app)) => clip
                .meta
                .source_app
                .as_deref()
                .is_some_and(|source| source.eq_ignore_ascii_case(app)),
            Some(SearchScope::Device(device)) => clip
                .meta
                .lineage
                .origin_device
                .as_deref()
                .is_some_and(|source| source.eq_ignore_ascii_case(device)),
            Some(SearchScope::Collection(collection)) => {
                tags.is_some_and(|tags| tags.has_collection(clip.id, collection))
            }
        }
    }
}

#[derive(Clone, Default)]
pub struct ClipTags {
    tags: HashMap<ClipId, BTreeSet<String>>,
    collections: HashMap<ClipId, BTreeSet<String>>,
    total: usize,
}

impl fmt::Debug for ClipTags {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClipTags")
            .field("clip_count", &self.tags.len().max(self.collections.len()))
            .field("label_count", &self.total)
            .finish()
    }
}

impl ClipTags {
    pub fn add_tag(&mut self, clip_id: ClipId, tag: &str) -> bool {
        self.add(clip_id, tag, false)
    }

    pub fn add_collection(&mut self, clip_id: ClipId, collection: &str) -> bool {
        self.add(clip_id, collection, true)
    }

    fn add(&mut self, clip_id: ClipId, value: &str, collection: bool) -> bool {
        let value = normalize_label(value);
        if !valid_label(&value, MAX_ALIAS_BYTES) || self.total >= MAX_TAGS {
            return false;
        }
        let map = if collection {
            &mut self.collections
        } else {
            &mut self.tags
        };
        let values = map.entry(clip_id).or_default();
        if values.len() >= MAX_TAGS_PER_CLIP || !values.insert(value) {
            return false;
        }
        self.total += 1;
        true
    }

    pub fn has_tag(&self, clip_id: ClipId, tag: &str) -> bool {
        let tag = normalize_label(tag);
        self.tags
            .get(&clip_id)
            .is_some_and(|tags| tags.contains(&tag))
    }

    pub fn has_collection(&self, clip_id: ClipId, collection: &str) -> bool {
        let collection = normalize_label(collection);
        self.collections
            .get(&clip_id)
            .is_some_and(|collections| collections.contains(&collection))
    }
}

#[derive(Clone, Default)]
pub struct QueryPinSet {
    pins: BTreeMap<[u8; 32], HashSet<ClipId>>,
}

impl fmt::Debug for QueryPinSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("QueryPinSet")
            .field("query_count", &self.pins.len())
            .field(
                "pin_count",
                &self.pins.values().map(HashSet::len).sum::<usize>(),
            )
            .finish()
    }
}

impl QueryPinSet {
    pub fn set(&mut self, query: [u8; 32], clip_id: ClipId, pinned: bool) -> bool {
        if query == [0; 32] {
            return false;
        }
        if pinned {
            if !self.pins.contains_key(&query) && self.pins.len() >= MAX_QUERY_PIN_GROUPS {
                return false;
            }
            let pins = self.pins.entry(query).or_default();
            if pins.len() >= MAX_QUERY_PINS && !pins.contains(&clip_id) {
                return false;
            }
            pins.insert(clip_id)
        } else {
            let Some(pins) = self.pins.get_mut(&query) else {
                return false;
            };
            let removed = pins.remove(&clip_id);
            if pins.is_empty() {
                self.pins.remove(&query);
            }
            removed
        }
    }

    pub fn contains(&self, query: [u8; 32], clip_id: ClipId) -> bool {
        self.pins
            .get(&query)
            .is_some_and(|pins| pins.contains(&clip_id))
    }
}

fn normalize_label(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn valid_macro_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_MACRO_NAME_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_label(value: &str, maximum_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum_bytes && !value.chars().any(char::is_control)
}

fn identity_hash(value: &str, maximum_bytes: usize) -> Option<[u8; 32]> {
    valid_label(value, maximum_bytes).then(|| *blake3::hash(value.as_bytes()).as_bytes())
}

fn query_looks_sensitive(query: &str) -> bool {
    let trimmed = query.trim();
    !detect_secrets(trimmed).is_empty()
        || ((4..=8).contains(&trimmed.len()) && trimmed.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use vbuff_types::{ClipMeta, ContentKind, Flavor};

    use super::*;

    fn clip(app: &str) -> Clip {
        Clip {
            id: ClipId::new(),
            flavors: vec![Flavor::inline("text/plain", b"hello".to_vec())],
            content_hash: [3; 32],
            meta: ClipMeta::now(ContentKind::Text, 5, Some(app.into())),
            pinned: true,
            favorite: false,
        }
    }

    #[test]
    fn macros_expand_with_cycle_and_size_guards() {
        let mut macros = SearchMacroRegistry::default();
        assert!(macros.set("work-links", "kind:url app:browser"));
        assert_eq!(
            macros.expand("@work-links today").as_deref(),
            Some("kind:url app:browser today")
        );
        assert!(macros.set("a", "@b"));
        assert!(macros.set("b", "@a"));
        assert_eq!(macros.expand("@a"), None);
        assert!(!format!("{macros:?}").contains("browser"));
    }

    #[test]
    fn aliases_affinity_and_query_pins_are_bounded_sidecars() {
        let clip = clip("editor");
        let mut aliases = PinnedAliases::default();
        assert!(aliases.add(clip.id, clip.pinned, "deploy command"));
        assert_eq!(aliases.match_score(clip.id, "deploy"), Some(180));

        let mut affinity = PasteAffinity::default();
        assert!(affinity.record("terminal", clip.content_hash));
        assert_eq!(affinity.boost("terminal", clip.content_hash), 4);
        assert!(!format!("{affinity:?}").contains("terminal"));

        let mut pins = QueryPinSet::default();
        assert!(pins.set([9; 32], clip.id, true));
        assert!(pins.contains([9; 32], clip.id));
        assert!(pins.set([9; 32], clip.id, false));
    }

    #[test]
    fn query_history_refuses_secrets_and_scope_lock_is_explicit() {
        let mut history = QueryHistory::new(2);
        assert!(history.remember("urls from browser"));
        assert!(!history.remember("ghp_abcdefghijklmnopqrstuvwxyz123456"));
        assert_eq!(
            history.entries().collect::<Vec<_>>(),
            vec!["urls from browser"]
        );

        let clip = clip("editor");
        let mut scope = SearchScopeLock::default();
        assert!(scope.set(SearchScope::App("editor".into())));
        assert!(scope.matches(&clip, None));
        scope.clear();
        assert!(scope.matches(&clip, None));
    }

    #[test]
    fn tags_and_collections_stay_separate() {
        let clip = clip("editor");
        let mut tags = ClipTags::default();
        assert!(tags.add_tag(clip.id, "urgent"));
        assert!(tags.add_collection(clip.id, "work"));
        assert!(tags.has_tag(clip.id, "URGENT"));
        assert!(tags.has_collection(clip.id, "work"));
        assert!(!tags.has_tag(clip.id, "work"));
    }
}
