//! Range selection, aggregate previews, and filter-by-example contracts.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;

use vbuff_types::{Clip, ClipId, ContentKind};

const MAX_SELECTION: usize = 512;
const MAX_AGGREGATE_PREVIEW_BYTES: usize = 64 * 1024;
const MAX_TAGS: usize = 64;
const MAX_TAG_BYTES: usize = 80;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RangeSelection {
    anchor: Option<usize>,
    selected: Vec<ClipId>,
}

impl RangeSelection {
    pub fn selected(&self) -> &[ClipId] {
        &self.selected
    }

    pub fn clear(&mut self) {
        self.anchor = None;
        self.selected.clear();
    }

    pub fn select(&mut self, ordered: &[ClipId], index: usize, extend: bool, toggle: bool) -> bool {
        let Some(id) = ordered.get(index).copied() else {
            return false;
        };
        if extend {
            let start = self.anchor.unwrap_or(index).min(index);
            let end = self.anchor.unwrap_or(index).max(index);
            if !toggle {
                self.selected.clear();
            }
            for candidate in ordered
                .iter()
                .skip(start)
                .take(end - start + 1)
                .take(MAX_SELECTION)
            {
                if !self.selected.contains(candidate) && self.selected.len() < MAX_SELECTION {
                    self.selected.push(*candidate);
                }
            }
        } else if toggle {
            if let Some(position) = self.selected.iter().position(|candidate| *candidate == id) {
                self.selected.remove(position);
            } else if self.selected.len() < MAX_SELECTION {
                self.selected.push(id);
            }
            self.anchor = Some(index);
        } else {
            self.selected.clear();
            self.selected.push(id);
            self.anchor = Some(index);
        }
        true
    }

    pub fn retain_visible(&mut self, ordered: &[ClipId]) {
        let visible = ordered.iter().copied().collect::<HashSet<_>>();
        self.selected.retain(|id| visible.contains(id));
        if self.anchor.is_some_and(|index| index >= ordered.len()) {
            self.anchor = None;
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SelectionAggregate {
    pub count: usize,
    pub byte_size: u64,
    pub kinds: BTreeSet<ContentKind>,
    pub text_count: usize,
    pub sensitive_count: usize,
    merged_preview: String,
    pub preview_truncated: bool,
}

impl fmt::Debug for SelectionAggregate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SelectionAggregate")
            .field("count", &self.count)
            .field("byte_size", &self.byte_size)
            .field("kinds", &self.kinds)
            .field("text_count", &self.text_count)
            .field("sensitive_count", &self.sensitive_count)
            .field(
                "merged_preview",
                &format_args!("[redacted; {} bytes]", self.merged_preview.len()),
            )
            .field("preview_truncated", &self.preview_truncated)
            .finish()
    }
}

impl SelectionAggregate {
    pub fn build(selected: &[ClipId], clips: &[Clip], preview_bytes: usize) -> Self {
        let preview_limit = preview_bytes.min(MAX_AGGREGATE_PREVIEW_BYTES);
        let by_id = clips
            .iter()
            .map(|clip| (clip.id, clip))
            .collect::<HashMap<_, _>>();
        let mut aggregate = Self {
            count: 0,
            byte_size: 0,
            kinds: BTreeSet::new(),
            text_count: 0,
            sensitive_count: 0,
            merged_preview: String::new(),
            preview_truncated: false,
        };
        for id in selected.iter().take(MAX_SELECTION) {
            let Some(clip) = by_id.get(id) else {
                continue;
            };
            aggregate.count += 1;
            aggregate.byte_size = aggregate.byte_size.saturating_add(clip.meta.byte_size);
            aggregate.kinds.insert(clip.meta.kind);
            aggregate.sensitive_count += usize::from(clip.meta.sensitive);
            if clip.meta.sensitive {
                continue;
            }
            let Some(text) = clip.primary_text() else {
                continue;
            };
            aggregate.text_count += 1;
            let separator = if aggregate.merged_preview.is_empty() {
                ""
            } else {
                "\n---\n"
            };
            let remaining = preview_limit.saturating_sub(aggregate.merged_preview.len());
            if separator.len().saturating_add(text.len()) <= remaining {
                aggregate.merged_preview.push_str(separator);
                aggregate.merged_preview.push_str(text);
            } else {
                aggregate.preview_truncated = true;
                if remaining > separator.len() {
                    aggregate.merged_preview.push_str(separator);
                    push_utf8_prefix(
                        &mut aggregate.merged_preview,
                        text,
                        remaining - separator.len(),
                    );
                }
            }
        }
        if selected.len() > MAX_SELECTION {
            aggregate.preview_truncated = true;
        }
        aggregate
    }

    pub fn merged_preview(&self) -> &str {
        &self.merged_preview
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ExampleFilter {
    pub kind: ContentKind,
    pub source_app: Option<String>,
    pub host: Option<String>,
    pub tags: BTreeSet<String>,
}

impl fmt::Debug for ExampleFilter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExampleFilter")
            .field("kind", &self.kind)
            .field(
                "source_app",
                &self.source_app.as_ref().map(|_| "[redacted]"),
            )
            .field("host", &self.host.as_ref().map(|_| "[redacted]"))
            .field("tag_count", &self.tags.len())
            .finish()
    }
}

impl ExampleFilter {
    pub fn matches(&self, clip: &Clip, clip_tags: &BTreeSet<String>) -> bool {
        if clip.meta.kind != self.kind || clip.meta.sensitive {
            return false;
        }
        if self
            .source_app
            .as_ref()
            .is_some_and(|expected| clip.meta.source_app.as_ref() != Some(expected))
        {
            return false;
        }
        if let Some(expected) = &self.host {
            let observed = clip
                .primary_text()
                .and_then(|text| url::Url::parse(text.trim()).ok())
                .and_then(|url| url.host_str().map(str::to_lowercase));
            if observed.as_ref() != Some(expected) {
                return false;
            }
        }
        self.tags.is_subset(clip_tags)
    }
}

pub fn filter_from_example(clip: &Clip, tags: &[String]) -> Option<ExampleFilter> {
    if clip.meta.sensitive
        || tags.len() > MAX_TAGS
        || tags.iter().any(|tag| {
            tag.trim().is_empty() || tag.len() > MAX_TAG_BYTES || tag.chars().any(char::is_control)
        })
    {
        return None;
    }
    let host = clip
        .primary_text()
        .and_then(|text| url::Url::parse(text.trim()).ok())
        .and_then(|url| url.host_str().map(str::to_lowercase));
    Some(ExampleFilter {
        kind: clip.meta.kind,
        source_app: clip.meta.source_app.clone(),
        host,
        tags: tags.iter().cloned().collect(),
    })
}

fn push_utf8_prefix(output: &mut String, value: &str, max_bytes: usize) {
    let mut boundary = max_bytes.min(value.len());
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    output.push_str(&value[..boundary]);
}

#[cfg(test)]
mod tests {
    use vbuff_types::{ClipMeta, Flavor};

    use super::*;

    fn clip(value: u128, text: &str, kind: ContentKind) -> Clip {
        let meta = ClipMeta::now(kind, text.len() as u64, Some("dev.editor".into()));
        Clip {
            id: ClipId(ulid::Ulid::from(value)),
            flavors: vec![Flavor::inline("text/plain", text.as_bytes().to_vec())],
            content_hash: [value as u8; 32],
            meta,
            pinned: false,
            favorite: false,
        }
    }

    #[test]
    fn shift_selection_and_aggregate_preview_are_bounded_and_redacted() {
        let clips = [
            clip(1, "alpha", ContentKind::Text),
            clip(2, "beta", ContentKind::Code),
            clip(3, "gamma", ContentKind::Text),
        ];
        let ordered = clips.iter().map(|clip| clip.id).collect::<Vec<_>>();
        let mut selection = RangeSelection::default();
        selection.select(&ordered, 0, false, false);
        selection.select(&ordered, 2, true, false);
        let aggregate = SelectionAggregate::build(selection.selected(), &clips, 12);
        assert_eq!(aggregate.count, 3);
        assert_eq!(aggregate.text_count, 3);
        assert!(aggregate.preview_truncated);
        assert!(aggregate.merged_preview().len() <= 12);
        assert!(!format!("{aggregate:?}").contains("alpha"));
    }

    #[test]
    fn filter_by_example_matches_kind_app_host_and_tags() {
        let source = clip(1, "https://docs.rs/rusqlite", ContentKind::Url);
        let filter = filter_from_example(&source, &["rust".into()]).unwrap();
        assert!(filter.matches(&source, &BTreeSet::from(["rust".into()])));
        assert!(!filter.matches(&source, &BTreeSet::new()));
        let other = clip(2, "https://example.test", ContentKind::Url);
        assert!(!filter.matches(&other, &BTreeSet::from(["rust".into()])));
        assert!(!format!("{filter:?}").contains("docs.rs"));
    }
}
